//! Inventory grids, free of Bevy: which cells each thing in a container covers, and whether
//! another thing fits. The rules are the plate's `fits`, `place` and `fitsIgnoring`
//! (`reference/forever-below-plate.html`): a thing covers a rectangle of cells from its
//! footprint, may lie turned a quarter turn, and may not overlap another or hang off the edge.

use fb_items::footprint::Footprint;

/// Where a thing lies in a grid: the top-left cell it covers, and whether it is turned a
/// quarter turn, so its footprint's columns and rows swap.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Placement {
    pub x: u8,
    pub y: u8,
    pub turned: bool,
}

impl Placement {
    pub fn new(x: u8, y: u8, turned: bool) -> Self {
        Placement { x, y, turned }
    }
}

/// `footprint` as it lies, turned or not.
pub fn oriented(footprint: Footprint, turned: bool) -> Footprint {
    if turned { Footprint { cols: footprint.rows, rows: footprint.cols } } else { footprint }
}

/// One thing in a grid: what it is, where it lies and the cells it covers from there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry<T> {
    pub id: T,
    pub at: Placement,
    /// The footprint as it lies ([`oriented`]).
    pub size: Footprint,
}

impl<T> Entry<T> {
    /// Whether it covers cell `(x, y)`.
    pub fn covers(&self, x: i32, y: i32) -> bool {
        let (x0, y0) = (self.at.x as i32, self.at.y as i32);
        x >= x0 && x < x0 + self.size.cols as i32 && y >= y0 && y < y0 + self.size.rows as i32
    }

    fn overlaps(&self, x: i32, y: i32, size: Footprint) -> bool {
        let (x0, y0) = (self.at.x as i32, self.at.y as i32);
        x < x0 + self.size.cols as i32 && x + size.cols as i32 > x0 && y < y0 + self.size.rows as i32 && y + size.rows as i32 > y0
    }
}

/// A container's cells and what lies in them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grid<T> {
    pub cols: u8,
    pub rows: u8,
    entries: Vec<Entry<T>>,
}

/// Why a thing cannot go where it was put.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Misfit {
    /// Some of it would hang off the grid.
    OffTheEdge,
    /// It would lie over something already there.
    Overlap,
}

impl<T: Copy + PartialEq> Grid<T> {
    pub fn new(cols: u8, rows: u8) -> Self {
        Grid { cols, rows, entries: Vec::new() }
    }

    /// What lies in it, in the order it was put there.
    pub fn entries(&self) -> &[Entry<T>] {
        &self.entries
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, id: T) -> Option<&Entry<T>> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// What covers cell `(x, y)`, if anything.
    pub fn at_cell(&self, x: i32, y: i32) -> Option<&Entry<T>> {
        self.entries.iter().find(|e| e.covers(x, y))
    }

    /// Whether something of `size` (as it lies) can go with its top-left cell at `(x, y)`,
    /// leaving `ignoring` out of the count: the thing being moved, so it can shift over its
    /// own old cells (the plate's `fitsIgnoring`).
    pub fn check(&self, size: Footprint, x: i32, y: i32, ignoring: Option<T>) -> Result<(), Misfit> {
        if x < 0 || y < 0 || x + size.cols as i32 > self.cols as i32 || y + size.rows as i32 > self.rows as i32 {
            return Err(Misfit::OffTheEdge);
        }
        if self.entries.iter().any(|e| Some(e.id) != ignoring && e.overlaps(x, y, size)) {
            return Err(Misfit::Overlap);
        }
        Ok(())
    }

    pub fn fits(&self, size: Footprint, x: i32, y: i32, ignoring: Option<T>) -> bool {
        self.check(size, x, y, ignoring).is_ok()
    }

    /// Lays `id` of `footprint` at `at`. If it is already in the grid it moves there, and may
    /// overlap its own old cells.
    pub fn place(&mut self, id: T, footprint: Footprint, at: Placement) -> Result<(), Misfit> {
        let size = oriented(footprint, at.turned);
        self.check(size, at.x as i32, at.y as i32, Some(id))?;
        self.entries.retain(|e| e.id != id);
        self.entries.push(Entry { id, at, size });
        Ok(())
    }

    /// The first free spot for `footprint`, reading the cells row by row, upright first and
    /// then turned, as the plate's `place` looks for room.
    pub fn first_free(&self, footprint: Footprint) -> Option<Placement> {
        [false, true].into_iter().find_map(|turned| {
            let size = oriented(footprint, turned);
            (0..self.rows).flat_map(|y| (0..self.cols).map(move |x| (x, y))).find_map(|(x, y)| {
                self.fits(size, x as i32, y as i32, None).then_some(Placement { x, y, turned })
            })
        })
    }

    /// Takes `id` out, returning where it lay.
    pub fn remove(&mut self, id: T) -> Option<Entry<T>> {
        let index = self.entries.iter().position(|e| e.id == id)?;
        Some(self.entries.remove(index))
    }

    /// A grid `cols` wide holding every one of `things` at the first free spot, in order, at
    /// least `min_rows` deep and as much deeper as they need. Something wider than `cols` even
    /// turned is left out. Used to lay out what lies underfoot.
    pub fn pack(cols: u8, min_rows: u8, things: impl IntoIterator<Item = (T, Footprint)>) -> Grid<T> {
        let mut grid = Grid::new(cols, min_rows);
        for (id, footprint) in things {
            if footprint.cols.min(footprint.rows) > cols {
                continue;
            }
            loop {
                if let Some(at) = grid.first_free(footprint) {
                    grid.place(id, footprint, at).expect("a free spot fits");
                    break;
                }
                grid.rows += 1;
            }
        }
        grid
    }
}

/// The cell under a point `offset` from a grid's top-left corner, with cells `cell` apart.
/// Can be outside the grid, or negative.
pub fn cell_at(offset: (f32, f32), cell: f32) -> (i32, i32) {
    ((offset.0 / cell).floor() as i32, (offset.1 / cell).floor() as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fp(cols: u8, rows: u8) -> Footprint {
        Footprint { cols, rows }
    }

    #[test]
    fn a_dagger_fits_any_free_spot_in_a_backpack() {
        // An 8 by 5 backpack and a 1 by 2 dagger.
        let pack: Grid<u32> = Grid::new(8, 5);
        for y in 0..4 {
            for x in 0..8 {
                assert!(pack.fits(fp(1, 2), x, y, None), "({x}, {y})");
            }
        }
        // Not hanging off the bottom or the side.
        assert_eq!(pack.check(fp(1, 2), 0, 4, None), Err(Misfit::OffTheEdge));
        assert_eq!(pack.check(fp(1, 2), 8, 0, None), Err(Misfit::OffTheEdge));
        assert_eq!(pack.check(fp(1, 2), -1, 0, None), Err(Misfit::OffTheEdge));
        // Turned it lies across two columns, so fits the bottom row.
        let mut pack = pack;
        pack.place(1, fp(1, 2), Placement::new(6, 4, true)).unwrap();
        assert_eq!(pack.get(1).unwrap().size, fp(2, 1));
    }

    #[test]
    fn an_overlapping_drop_is_refused() {
        let mut pack: Grid<u32> = Grid::new(8, 5);
        pack.place(1, fp(1, 2), Placement::new(3, 1, false)).unwrap();
        assert_eq!(pack.place(2, fp(2, 2), Placement::new(2, 2, false)), Err(Misfit::Overlap));
        assert_eq!(pack.entries().len(), 1);
        // Beside it is fine.
        pack.place(2, fp(2, 2), Placement::new(4, 1, false)).unwrap();
        // A thing moved can shift over its own old cells.
        pack.place(1, fp(1, 2), Placement::new(3, 2, false)).unwrap();
        assert_eq!(pack.get(1).unwrap().at, Placement::new(3, 2, false));
        assert_eq!(pack.entries().len(), 2);
    }

    #[test]
    fn the_first_free_spot_reads_row_by_row_then_turns() {
        let mut pouch: Grid<u32> = Grid::new(4, 2);
        assert_eq!(pouch.first_free(fp(2, 2)), Some(Placement::new(0, 0, false)));
        pouch.place(1, fp(2, 2), Placement::new(0, 0, false)).unwrap();
        assert_eq!(pouch.first_free(fp(1, 1)), Some(Placement::new(2, 0, false)));
        // A 1 by 3 thing only fits a 4 by 2 pouch lying turned.
        let empty: Grid<u32> = Grid::new(4, 2);
        assert_eq!(empty.first_free(fp(1, 3)), Some(Placement::new(0, 0, true)));
        pouch.place(2, fp(2, 2), Placement::new(2, 0, false)).unwrap();
        assert_eq!(pouch.first_free(fp(1, 1)), None);
    }

    #[test]
    fn cells_report_what_covers_them() {
        let mut grid: Grid<u32> = Grid::new(6, 3);
        grid.place(7, fp(2, 3), Placement::new(1, 0, false)).unwrap();
        assert_eq!(grid.at_cell(2, 2).map(|e| e.id), Some(7));
        assert_eq!(grid.at_cell(3, 0), None);
        assert_eq!(grid.remove(7).map(|e| e.at), Some(Placement::new(1, 0, false)));
        assert!(grid.is_empty());
        assert_eq!(cell_at((47.0, 93.0), 46.0), (1, 2));
        assert_eq!(cell_at((-3.0, 10.0), 46.0), (-1, 0));
    }

    #[test]
    fn packing_grows_deep_enough_for_everything() {
        let things = [(1, fp(1, 1)), (2, fp(1, 7)), (3, fp(2, 2)), (4, fp(9, 9))];
        let grid = Grid::pack(4, 3, things);
        // The 9 by 9 thing never fits four columns; the rest all lie somewhere, apart.
        assert_eq!(grid.entries().len(), 3);
        assert!(grid.rows >= 3);
        for e in grid.entries() {
            assert!(e.at.x as i32 + e.size.cols as i32 <= 4 && e.at.y as i32 + e.size.rows as i32 <= grid.rows as i32);
        }
        // Seven long, too long to turn across four columns: the grid grows seven deep for it.
        assert_eq!((grid.get(2).unwrap().size, grid.rows), (fp(1, 7), 7));
    }
}
