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
    // one thread: no pool, and none left by a render this one runs inside of
    let pool = (threads > 1)
        .then(|| {
            ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .map_err(|error| Error::Recipe(format!("could not start {threads} threads: {error}")))
        })
        .transpose()?;
    let _restore = Restore(POOL.replace(pool));
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
