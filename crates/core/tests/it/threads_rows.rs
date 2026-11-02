//! How an effect that works pixel by pixel uses the threads a render was given, seen through `Picture::map_at`, the public
//! door to the row bands: the pixels come out as if made on one thread at any thread count, a render of one thread inside
//! a render of many runs on the calling thread alone, and a pool lasts as long as the render that asked for it.

use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, ThreadId};

use bullterrier_image::{
    Category, Effect, Error, Image, Inputs, Options, Photo, Picture, Presets, Recipe, Step, register,
};

type Outcome = Result<(), Box<dyn std::error::Error>>;

/// An effect that paints the number of each pixel (row times the width plus the column) in all three channels, and
/// remembers the threads it painted on. Given an `inner` effect it first renders a picture of its own with that, on one thread.
struct Numbered {
    name: String,
    threads: Mutex<HashSet<ThreadId>>,
    /// The threads the effect it renders inside (if any) painted on.
    inner: Option<Arc<Numbered>>,
    /// What `map_at` did on the calling thread of the render once the inner render had finished, if it has one.
    after: Mutex<HashSet<ThreadId>>,
}

impl Numbered {
    fn register(inner: Option<Arc<Self>>) -> Result<Arc<Self>, Error> {
        static EFFECTS: AtomicUsize = AtomicUsize::new(0);
        let effect = Arc::new(Self {
            name: format!("threads_rows_{}", EFFECTS.fetch_add(1, Ordering::Relaxed)),
            threads: Mutex::new(HashSet::new()),
            inner,
            after: Mutex::new(HashSet::new()),
        });
        register(effect.clone())?;
        Ok(effect)
    }

    fn step(&self) -> Step {
        Step::new(self.name.as_str()).opacity(1.0)
    }

    fn painted_on(&self) -> HashSet<ThreadId> {
        self.threads.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    /// The pixels of `picture` numbered, each row noting the thread it was made on in `seen`.
    fn number(picture: &Picture<'_>, seen: &Mutex<HashSet<ThreadId>>) -> Image {
        let width = picture.width();
        picture.map_at(|row, column, _| {
            if column == 0 {
                seen.lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .insert(thread::current().id());
            }
            [small(row * width + column); 3]
        })
    }
}

impl Effect for Numbered {
    fn name(&self) -> &str {
        &self.name
    }

    fn category(&self) -> Category {
        Category::Utilities
    }

    fn apply(&self, picture: &Picture<'_>, _params: serde_json::Value, _inputs: &Inputs<'_>) -> Result<Image, Error> {
        if let Some(inner) = &self.inner {
            // a render of one thread (the default), inside the render this effect is running in
            let values = vec![0.5; ROWS * WIDTH * 3];
            Photo::new(Picture::new(ROWS, WIDTH, &values)?).render(
                &Recipe::new([inner.step()]),
                &Presets::new(),
                Options::default(),
            )?;
            // and back in the outer render: its own bands again
            return Ok(Self::number(picture, &self.after));
        }
        Ok(Self::number(picture, &self.threads))
    }
}

const ROWS: usize = 1000;
const WIDTH: usize = 40;

/// A pixel number as the f32 it is painted as: below 2^24, so exact.
#[expect(clippy::cast_precision_loss, reason = "a pixel number here is far below 2^24")]
const fn small(number: usize) -> f32 {
    number as f32
}

/// The effect rendered on a grey photo `height` rows tall with `threads`.
fn render(effect: &Numbered, height: usize, threads: usize) -> Result<Vec<f32>, Error> {
    let values = vec![0.5; height * WIDTH * 3];
    Ok(Photo::new(Picture::new(height, WIDTH, &values)?)
        .render(
            &Recipe::new([effect.step()]),
            &Presets::new(),
            Options::default().with_threads(threads),
        )?
        .into_values())
}

#[test]
fn rows_that_depend_only_on_their_position_come_out_the_same_at_any_thread_count() -> Outcome {
    let expected: Vec<f32> = (0..ROWS * WIDTH).flat_map(|at| [small(at); 3]).collect();
    for threads in [0, 1, 2, 3, 10] {
        let effect = Numbered::register(None)?;
        let out = render(&effect, ROWS, threads)?;
        assert!(out == expected, "{threads} threads");
    }
    Ok(())
}

#[test]
fn a_render_of_many_threads_makes_its_rows_on_the_pool_and_one_thread_makes_them_on_the_caller() -> Outcome {
    let caller = thread::current().id();
    let one = Numbered::register(None)?;
    render(&one, ROWS, 1)?;
    assert_eq!(one.painted_on(), HashSet::from([caller]));
    let many = Numbered::register(None)?;
    render(&many, ROWS, 4)?;
    let pool = many.painted_on();
    assert!(!pool.is_empty() && !pool.contains(&caller), "{pool:?}");
    Ok(())
}

#[test]
fn a_render_of_one_thread_inside_a_render_of_more_runs_its_bands_on_the_calling_thread_alone() -> Outcome {
    let caller = thread::current().id();
    let inner = Numbered::register(None)?;
    let outer = Numbered::register(Some(inner.clone()))?;
    render(&outer, ROWS, 4)?;
    assert_eq!(
        inner.painted_on(),
        HashSet::from([caller]),
        "a band ran on the outer render's pool"
    );
    Ok(())
}

#[test]
fn a_nested_render_puts_the_outer_pool_back_when_it_ends() -> Outcome {
    let caller = thread::current().id();
    let inner = Numbered::register(None)?;
    let outer = Numbered::register(Some(inner))?;
    // the inner render has one thread; after it the outer effect paints on the pool of 2 again, not on the caller
    render(&outer, 20_000, 2)?;
    let after = outer.after.lock().unwrap_or_else(PoisonError::into_inner).clone();
    assert!(
        !after.is_empty() && !after.contains(&caller) && after.len() <= 2,
        "{after:?}"
    );
    Ok(())
}

#[test]
fn a_pool_lasts_for_its_render_only() -> Outcome {
    let effect = Numbered::register(None)?;
    render(&effect, ROWS, 2)?;
    // a bare map_at on this thread, after the render, makes its rows here
    let seen = Mutex::new(HashSet::new());
    let values = vec![0.5; ROWS * WIDTH * 3];
    let _ = Numbered::number(&Picture::new(ROWS, WIDTH, &values)?, &seen);
    assert_eq!(
        seen.into_inner().unwrap_or_else(PoisonError::into_inner),
        HashSet::from([thread::current().id()])
    );
    Ok(())
}
