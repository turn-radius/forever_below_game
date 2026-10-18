//! Fitting gear to a body takes most of a second (`fb_body::fit_worn` fits every worn layer),
//! far too long for a frame. A [`Fitting`] runs one such job on the async compute pool and keeps
//! the key it was started for, so the system that started it can poll it each frame, drop it if
//! the key has moved on, and swap in the result when it finishes while what was drawn before
//! stays on screen.

use std::fmt;

use bevy::tasks::{AsyncComputeTaskPool, Task, TaskPool, block_on};

/// A job running off the main thread, made for `key`. Dropping the [`Fitting`] cancels the task
/// if it hasn't started yet; once running, the task finishes but its result is discarded.
pub struct Fitting<K, T> {
    key: K,
    task: Option<Task<T>>,
}

impl<K, T: Send + 'static> Fitting<K, T> {
    /// Starts `work` on the async compute pool.
    pub fn start(key: K, work: impl FnOnce() -> T + Send + 'static) -> Self {
        let pool = AsyncComputeTaskPool::get_or_init(TaskPool::default);
        Fitting { key, task: Some(pool.spawn(async move { work() })) }
    }

    /// What the job was started for.
    pub fn key(&self) -> &K {
        &self.key
    }

    /// The result, waiting for the job if it has not finished. `None` if it was already taken.
    pub fn wait(&mut self) -> Option<T> {
        self.task.take().map(block_on)
    }

    /// The result, if the job has finished. Takes it once: call again and it is `None`.
    pub fn poll(&mut self) -> Option<T> {
        let task = self.task.as_ref()?;
        if task.is_finished() {
            // Take the task and return its result
            self.task.take().map(|t| block_on(t))
        } else {
            None
        }
    }
}

impl<K: fmt::Debug, T> fmt::Debug for Fitting<K, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Fitting").field("key", &self.key).finish_non_exhaustive()
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
}
