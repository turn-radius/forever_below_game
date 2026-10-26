//! Fitting gear to a body takes most of a second (`fb_body::fit_worn` fits every worn layer),
//! far too long for a frame. A [`Fitting`] runs one such job on the async compute pool and keeps
//! the key it was started for, so the system that started it can poll it each frame, drop it if
//! the key has moved on, and swap in the result when it finishes while what was drawn before
//! stays on screen.
//!
//! What has been fitted is kept in a [`FitCache`], so putting on something worn or held a
//! moment ago shows it at once: whoever starts a fit looks there first and, on a hit, takes
//! [`Fitting::ready`] in place of a job.

use std::fmt;
use std::sync::Arc;

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, TaskPool, block_on};
use fb_body::BodySpec;
use fb_items::{Item, Slot};

use crate::carry::HeldFit;
use crate::figure::Figure;

/// How many fits the [`FitCache`] keeps: outfits and held kits together, the least recently
/// used dropped first.
pub const FIT_CACHE_LIMIT: usize = 16;

/// A job running off the main thread, made for `key`. Dropping the [`Fitting`] cancels the task
/// if it hasn't started yet; once running, the task finishes but its result is discarded.
pub struct Fitting<K, T> {
    key: K,
    task: Option<Task<T>>,
    /// A result already in hand (from the cache), given by the first poll in place of a task.
    ready: Option<T>,
}

impl<K, T: Send + 'static> Fitting<K, T> {
    /// Starts `work` on the async compute pool.
    pub fn start(key: K, work: impl FnOnce() -> T + Send + 'static) -> Self {
        let pool = AsyncComputeTaskPool::get_or_init(TaskPool::default);
        Fitting {
            key,
            task: Some(pool.spawn(async move { work() })),
            ready: None,
        }
    }

    /// A job that is done already, holding `result`: no task is spawned.
    pub fn ready(key: K, result: T) -> Self {
        Fitting {
            key,
            task: None,
            ready: Some(result),
        }
    }

    /// What the job was started for.
    pub fn key(&self) -> &K {
        &self.key
    }

    /// The result, waiting for the job if it has not finished. `None` if it was already taken.
    pub fn wait(&mut self) -> Option<T> {
        self.ready.take().or_else(|| self.task.take().map(block_on))
    }

    /// The result, if the job has finished. Takes it once: call again and it is `None`.
    pub fn poll(&mut self) -> Option<T> {
        if let Some(result) = self.ready.take() {
            return Some(result);
        }
        let task = self.task.as_ref()?;
        if task.is_finished() {
            // Take the task and return its result
            self.task.take().map(|t| block_on(t))
        } else {
            None
        }
    }
}

/// A fit kept in the cache, shared so a hit does not copy its meshes.
#[derive(Clone)]
enum Fitted {
    Figure(Arc<Figure>),
    Held(Arc<HeldFit>),
}

/// What has been fitted lately: whole dressed figures (the doll's and the lab's) and held kits
/// (the view model's), by the key of everything that went into them ([`figure_key`],
/// [`crate::carry::held_key`]). Bounded by [`FIT_CACHE_LIMIT`], the least recently used out
/// first. A fit is a pure function of its key, so a hit is exactly what a fresh fit returns.
#[derive(Resource, Default)]
pub struct FitCache {
    /// Oldest use first.
    entries: Vec<(String, Fitted)>,
    /// How many looks found a fit, and how many did not (each of which starts a job).
    pub hits: usize,
    pub misses: usize,
}

impl FitCache {
    /// How many fits are kept.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn contains(&self, key: &str) -> bool {
        self.entries.iter().any(|(k, _)| k == key)
    }

    /// The figure fitted for `key`, if kept; it counts as used.
    pub fn figure(&mut self, key: &str) -> Option<Arc<Figure>> {
        match self.take(key, |f| matches!(f, Fitted::Figure(_))) {
            Some(Fitted::Figure(figure)) => Some(figure),
            _ => None,
        }
    }

    /// The held kit fitted for `key`, if kept; it counts as used.
    pub fn held(&mut self, key: &str) -> Option<Arc<HeldFit>> {
        match self.take(key, |f| matches!(f, Fitted::Held(_))) {
            Some(Fitted::Held(held)) => Some(held),
            _ => None,
        }
    }

    pub fn store_figure(&mut self, key: &str, figure: Arc<Figure>) {
        self.put(key, Fitted::Figure(figure));
    }

    pub fn store_held(&mut self, key: &str, held: Arc<HeldFit>) {
        self.put(key, Fitted::Held(held));
    }

    /// Looks `key` up for a fit of the `wanted` kind, counting the hit or miss and moving a hit
    /// to the newest end.
    fn take(&mut self, key: &str, wanted: fn(&Fitted) -> bool) -> Option<Fitted> {
        let Some(at) = self.entries.iter().position(|(k, f)| k == key && wanted(f)) else {
            self.misses += 1;
            return None;
        };
        self.hits += 1;
        let entry = self.entries.remove(at);
        let fitted = entry.1.clone();
        self.entries.push(entry);
        Some(fitted)
    }

    fn put(&mut self, key: &str, fitted: Fitted) {
        self.entries.retain(|(k, _)| k != key);
        self.entries.push((key.to_string(), fitted));
        if self.entries.len() > FIT_CACHE_LIMIT {
            self.entries.remove(0);
        }
    }
}

/// One worn or held item as it enters a key: its slot, base and seed, the name it is drawn
/// under, and its whole state (wear, a torch's fuel, a bow's arrow). An item's `Debug` prints
/// every field, floats at full precision, so two states that differ never share a key.
pub fn item_key(slot: Slot, item: &Item) -> String {
    format!(
        "{slot:?}/{}/{}/{}/{}",
        item.base,
        item.seed,
        item.title(),
        shape_key(&item.shape)
    )
}

/// A shape's whole state, as it enters a key.
pub fn shape_key(shape: &armory::Item) -> String {
    format!("{shape:?}")
}

/// Everything [`Figure::dress`] depends on: every field of `spec`, the dirt, and each worn
/// item ([`item_key`]) in order.
pub fn figure_key(spec: &BodySpec, worn: &[(Slot, Item)], dirt: f32) -> String {
    let mut key = format!("figure|{spec:?}|{}", dirt.to_bits());
    for (slot, item) in worn {
        key.push('|');
        key.push_str(&item_key(*slot, item));
    }
    key
}

impl<K: fmt::Debug, T> fmt::Debug for Fitting<K, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Fitting")
            .field("key", &self.key)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn a_job_is_polled_until_it_finishes() {
        let (go, wait) = mpsc::channel::<()>();
        let mut job = Fitting::start("key", move || {
            wait.recv().unwrap();
            7
        });
        assert_eq!(*job.key(), "key");
        assert_eq!(job.poll(), None, "held back, it has not finished");
        go.send(()).unwrap();
        let result = std::iter::repeat_with(|| {
            std::thread::sleep(std::time::Duration::from_millis(5));
            job.poll()
        })
        .take(1000)
        .flatten()
        .next();
        assert_eq!(result, Some(7));
    }

    #[test]
    fn a_ready_job_gives_its_result_at_once() {
        let mut job = Fitting::ready("key", 9);
        assert_eq!(job.poll(), Some(9));
        assert_eq!(job.poll(), None);
    }

    fn bare() -> Arc<Figure> {
        Arc::new(Figure::dress(
            &BodySpec::roll(fb_body::Folk::Human, 1),
            &[],
            0.0,
        ))
    }

    #[test]
    fn the_cache_never_exceeds_its_limit_and_drops_the_least_recently_used() {
        let figure = bare();
        let mut cache = FitCache::default();
        for n in 0..FIT_CACHE_LIMIT {
            cache.store_figure(&format!("k{n}"), figure.clone());
        }
        assert_eq!(cache.len(), FIT_CACHE_LIMIT);
        // Using the oldest makes the next oldest the one to go.
        assert!(cache.figure("k0").is_some());
        cache.store_figure("new", figure.clone());
        assert_eq!(cache.len(), FIT_CACHE_LIMIT);
        assert!(cache.contains("k0") && cache.contains("new") && !cache.contains("k1"));
        for n in 0..3 * FIT_CACHE_LIMIT {
            cache.store_figure(&format!("more{n}"), figure.clone());
            assert!(cache.len() <= FIT_CACHE_LIMIT);
        }
        // Storing a key again replaces it rather than adding a second.
        cache.store_figure("more47", figure);
        assert_eq!(cache.len(), FIT_CACHE_LIMIT);
    }

    #[test]
    fn a_hit_is_shared_not_copied() {
        let figure = bare();
        let mut cache = FitCache::default();
        cache.store_figure("k", figure.clone());
        assert!(Arc::ptr_eq(&cache.figure("k").unwrap(), &figure));
        assert!(cache.figure("other").is_none() && cache.held("k").is_none());
        assert_eq!(
            (cache.hits, cache.misses),
            (1, 2),
            "a held kit is not a figure"
        );
    }

    #[test]
    fn a_changed_item_state_is_another_key() {
        let spec = BodySpec::roll(fb_body::Folk::Human, 7);
        let mut helm = fb_items::roll_item("Helm", 5).unwrap();
        let key = figure_key(&spec, &[(Slot::Head, helm.clone())], 0.2);
        assert_eq!(key, figure_key(&spec, &[(Slot::Head, helm.clone())], 0.2));
        assert_ne!(
            key,
            figure_key(&spec, &[(Slot::Head, helm.clone())], 0.3),
            "dirt"
        );
        assert_ne!(
            key,
            figure_key(
                &BodySpec {
                    jaw: spec.jaw + 0.01,
                    ..spec.clone()
                },
                &[(Slot::Head, helm.clone())],
                0.2
            ),
            "a body field"
        );
        assert_ne!(
            key,
            figure_key(&spec, &[(Slot::Hip, helm.clone())], 0.2),
            "the slot"
        );
        let mut condition = helm.shape.condition();
        condition.wear += if condition.wear > 0.5 { -0.1 } else { 0.1 };
        helm.shape.set_condition(condition);
        let worn = figure_key(&spec, &[(Slot::Head, helm)], 0.2);
        assert_ne!(key, worn, "wear");
        let mut cache = FitCache::default();
        cache.store_figure(&key, bare());
        assert!(
            cache.figure(&worn).is_none(),
            "the worn helm is fitted again"
        );
    }

    #[test]
    fn a_cached_figure_is_what_a_fresh_dressing_makes() {
        let spec = BodySpec::roll(fb_body::Folk::Dwarf, 7);
        let worn = fb_items::outfit::outfit("fighter")
            .unwrap()
            .roll(7, fb_body::Folk::Dwarf.key())
            .worn;
        let key = figure_key(&spec, &worn, 0.3);
        let mut cache = FitCache::default();
        cache.store_figure(&key, Arc::new(Figure::dress(&spec, &worn, 0.3)));
        let cached = cache.figure(&key).unwrap();
        assert_eq!(cached.pieces, Figure::dress(&spec, &worn, 0.3).pieces);
    }

    #[test]
    fn leaving_the_delve_empties_the_cache() {
        use bevy::ecs::system::RunSystemOnce;
        let mut world = World::new();
        let mut cache = FitCache::default();
        cache.store_figure("k", bare());
        world.insert_resource(cache);
        world.init_resource::<crate::inventory::DollStage>();
        world.init_resource::<crate::inventory::icon::IconCache>();
        world
            .run_system_once(crate::inventory::leave_the_delve)
            .unwrap();
        assert!(world.resource::<FitCache>().is_empty());
    }
}
