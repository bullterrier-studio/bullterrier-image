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
use std::collections::BTreeMap;
use std::num::NonZero;
use std::sync::{Arc, Mutex, PoisonError};

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
    static POOL: RefCell<Option<Arc<ThreadPool>>> = const { RefCell::new(None) };
}

/// The pools of the process, one for each thread count a render asked for: started the first time and kept, so that renders
/// at the same count, however many run at once, share its threads and never start threads of their own.
static POOLS: Mutex<BTreeMap<usize, Arc<ThreadPool>>> = Mutex::new(BTreeMap::new());

/// The pool of `threads` threads (at least 2), started if no render has asked for one of that size yet.
fn shared(threads: usize) -> Result<Arc<ThreadPool>, Error> {
    let mut pools = POOLS.lock().unwrap_or_else(PoisonError::into_inner); // a pool map stays whole if a render panicked
    if let Some(pool) = pools.get(&threads) {
        return Ok(Arc::clone(pool));
    }
    let pool = ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .map_err(|error| Error::Recipe(format!("could not start {threads} threads: {error}")))?;
    Ok(Arc::clone(pools.entry(threads).or_insert(Arc::new(pool))))
}

/// Puts back, when dropped, the pool the thread had before an [`install`].
struct Restore(Option<Arc<ThreadPool>>);

impl Drop for Restore {
    fn drop(&mut self) {
        POOL.replace(self.0.take());
    }
}

/// `op` run on this thread with the pool of `threads` threads for [`rows`] to use (0: one for each core; 1: none, nothing
/// is spawned). The pool is shared by every render at that count in the process (see [`POOLS`]), so renders running at
/// once on several threads of the caller use `threads` threads in all, not that many each.
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
    let pool = (threads > 1).then(|| shared(threads)).transpose()?;
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

/// `fold(first, chunk)` for each chunk of `chunk_len` elements of `data` (the last may be shorter), in order, on the threads
/// of the render's pool when it has one. The chunks are cut by the data and `chunk_len` alone, never by the number of
/// threads, so a reduction of the results is the same at any thread count. `chunk_len` is at least 1.
pub(crate) fn chunks<T: Sync, R: Send>(
    data: &[T],
    chunk_len: usize,
    fold: impl Fn(usize, &[T]) -> R + Sync + Send,
) -> Vec<R> {
    POOL.with_borrow(|pool| match pool {
        Some(pool) => pool.install(|| {
            data.par_chunks(chunk_len)
                .enumerate()
                .map(|(chunk, part)| fold(chunk * chunk_len, part))
                .collect()
        }),
        None => data
            .chunks(chunk_len)
            .enumerate()
            .map(|(chunk, part)| fold(chunk * chunk_len, part))
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::{Mutex, PoisonError};
    use std::thread::{self, ThreadId};

    use super::*;

    /// The threads that ran bands in `renders` renders one after the other, each at `threads` threads and filling enough
    /// elements to give every thread bands.
    fn workers(threads: usize, renders: usize) -> Result<HashSet<ThreadId>, Error> {
        let seen = Mutex::new(HashSet::new());
        for _ in 0..renders {
            install(threads, || {
                let mut out = vec![0_u8; threads * BANDS_PER_THREAD * MIN_BAND];
                rows(&mut out, 1, |_, band| {
                    band.fill(1);
                    seen.lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .insert(thread::current().id());
                });
            })?;
        }
        Ok(seen.into_inner().unwrap_or_else(PoisonError::into_inner))
    }

    #[test]
    fn renders_at_the_same_thread_count_run_their_bands_on_the_same_threads() -> Result<(), Error> {
        let found = workers(3, 12)?;
        assert!(
            (1..=3).contains(&found.len()),
            "{} threads ran the bands of 12 renders at 3 threads",
            found.len()
        );
        Ok(())
    }

    #[test]
    fn a_render_at_fewer_threads_than_another_never_uses_more_than_its_own() -> Result<(), Error> {
        workers(3, 2)?;
        let found = workers(2, 6)?;
        assert!(
            found.len() <= 2,
            "{} threads ran the bands of renders at 2 threads",
            found.len()
        );
        Ok(())
    }

    #[test]
    fn renders_that_run_at_once_share_the_threads_of_their_count() -> Result<(), Error> {
        let seen = Mutex::new(HashSet::new());
        let fill = |_: usize, band: &mut [u8]| {
            band.fill(1);
            seen.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(thread::current().id());
        };
        thread::scope(|scope| {
            let renders: Vec<_> = (0..8)
                .map(|_| {
                    scope.spawn(|| {
                        install(4, || {
                            let mut out = vec![0_u8; 4 * BANDS_PER_THREAD * MIN_BAND];
                            rows(&mut out, 1, fill);
                        })
                    })
                })
                .collect();
            renders.into_iter().try_for_each(|render| {
                render
                    .join()
                    .map_err(|_| Error::Recipe("a render panicked".to_owned()))?
            })
        })?;
        let found = seen.into_inner().unwrap_or_else(PoisonError::into_inner).len();
        assert!(
            found <= 4,
            "{found} threads ran the bands of 8 renders at once at 4 threads"
        );
        Ok(())
    }
}
