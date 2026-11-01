//! Row bands: how an op uses the threads a render was given ([`Options::with_threads`](crate::Options::with_threads)).
//!
//! A render with more than one thread calls [`install`] around its work; an op that fills rows of output independently
//! calls [`rows`], which hands bands of whole rows to the pool's threads. Outside an install, and for a render of one
//! thread, [`rows`] runs the op on the calling thread, so no thread is spawned. An op's output must not depend on the
//! bands, which is why [`rows`] hands out only whole rows and no op may carry anything from one row to the next: the same
//! output at any thread count is the rule of the core, tested for every op that uses this module.
//!
//! The render itself stays on the thread that called it (a Python function an effect runs there keeps its context);
//! the pool's threads run only the bands.

use std::cell::RefCell;
use std::num::NonZero;

use rayon::prelude::*;
use rayon::{ThreadPool, ThreadPoolBuilder};

use crate::Error;

/// The most threads a render may ask for.
pub(crate) const MOST_THREADS: usize = 256; // constant: far above any machine's cores; a typo such as 10000 is refused

/// The fewest elements of output a band holds: below this, the cost of handing a band over outweighs the work in it.
const MIN_BAND: usize = 1 << 14; // constant: about 60 us in the cheapest op (laying a step, 4 ns a pixel), a few times a pool's 10 to 20 us wake-up (M4)

/// How many bands each thread gets, so that a thread that finishes early takes another band.
const BANDS_PER_THREAD: usize = 4; // constant: enough to even out threads of unequal speed, few enough to stay coarse

thread_local! {
    /// The pool of the render running on this thread, if it has more than one thread.
    static POOL: RefCell<Option<ThreadPool>> = const { RefCell::new(None) };
}

/// Puts back, when dropped, the pool the thread had before an [`install`].
struct Restore(Option<ThreadPool>);

impl Drop for Restore {
    fn drop(&mut self) {
        POOL.replace(self.0.take());
    }
}

/// `op` run on this thread with a pool of `threads` threads for [`rows`] to use (0: one for each core; 1: none, nothing
/// is spawned).
///
/// # Errors
/// [`Error::Recipe`] for more than [`MOST_THREADS`] threads, naming the number, or if the system refuses the threads.
pub(crate) fn install<R>(threads: usize, op: impl FnOnce() -> R) -> Result<R, Error> {
    if threads > MOST_THREADS {
        return Err(Error::Recipe(format!(
            "`threads` must be from 0 to {MOST_THREADS}, got {threads}"
        )));
    }
    // 0 asks for every core; a system that cannot tell has one
    let threads = if threads == 0 {
        std::thread::available_parallelism().map_or(1, NonZero::get)
    } else {
        threads
    };
    if threads == 1 {
        return Ok(op());
    }
    let pool = ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .map_err(|error| Error::Recipe(format!("could not start {threads} threads: {error}")))?;
    let _restore = Restore(POOL.replace(Some(pool)));
    Ok(op())
}

/// `out`, rows of `row_len` elements, filled by `fill(first_row, band)` for bands of whole rows that together are all of
/// `out`, on the threads of the render's pool when it has one. `fill` writes only its band and must give the same
/// elements however the rows are cut into bands. `out` has at least one row, and `row_len` is at least 1.
pub(crate) fn rows<T: Send>(out: &mut [T], row_len: usize, fill: impl Fn(usize, &mut [T]) + Sync) {
    POOL.with_borrow(|pool| {
        let Some(pool) = pool else {
            return fill(0, out);
        };
        let rows = out.len() / row_len;
        let per_band = rows
            .div_ceil(pool.current_num_threads() * BANDS_PER_THREAD)
            .max(MIN_BAND.div_ceil(row_len));
        pool.install(|| {
            out.par_chunks_mut(per_band * row_len)
                .enumerate()
                .for_each(|(band, chunk)| fill(band * per_band, chunk));
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 1000 rows of 40 elements, each element its row number times 40 plus its column: the same however it is made.
    fn filled(threads: usize, fill: impl Fn(usize, &mut [usize]) + Sync) -> Result<Vec<usize>, Error> {
        let mut out = vec![0; 1000 * 40];
        install(threads, || rows(&mut out, 40, &fill))?;
        Ok(out)
    }

    fn by_position(first_row: usize, band: &mut [usize]) {
        for (index, element) in band.iter_mut().enumerate() {
            *element = first_row * 40 + index;
        }
    }

    #[test]
    fn rows_that_depend_only_on_their_position_come_out_the_same_at_any_thread_count() -> Result<(), Error> {
        let expected: Vec<usize> = (0..1000 * 40).collect();
        for threads in [0, 1, 2, 3, 10] {
            assert!(filled(threads, by_position)? == expected, "{threads} threads");
        }
        Ok(())
    }

    #[test]
    fn rows_that_know_only_their_place_in_a_band_differ_between_thread_counts_which_the_comparison_sees()
    -> Result<(), Error> {
        // the known-bad op: each element is its index within its band, not within the picture, so it depends on the cuts
        let within_band = |_: usize, band: &mut [usize]| {
            for (index, element) in band.iter_mut().enumerate() {
                *element = index;
            }
        };
        assert!(
            filled(1, within_band)? != filled(10, within_band)?,
            "one band and many came out the same"
        );
        Ok(())
    }

    #[test]
    fn a_thread_count_nobody_has_is_refused_naming_it() {
        let refused = install(MOST_THREADS + 1, || ());
        assert!(
            matches!(&refused, Err(Error::Recipe(said)) if said.contains(&(MOST_THREADS + 1).to_string())),
            "{refused:?}"
        );
        assert!(install(MOST_THREADS, || ()).is_ok());
    }

    #[test]
    fn a_pool_lasts_for_its_install_only() -> Result<(), Error> {
        install(2, || ())?;
        assert!(POOL.with_borrow(Option::is_none));
        // a nested install puts the outer pool back
        install(2, || -> Result<(), Error> {
            install(3, || ())?;
            let size = POOL.with_borrow(|pool| pool.as_ref().map(ThreadPool::current_num_threads));
            assert_eq!(size, Some(2));
            Ok(())
        })??;
        Ok(())
    }
}
