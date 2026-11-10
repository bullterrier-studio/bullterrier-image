//! Running the steps of a recipe in bands of rows: [`Run::advance_banded`].

use super::Run;
use super::step::Prepared;
use crate::{Canvas, Error, Image, Picture};

impl Run<'_> {
    /// Runs every step still to run, in bands of `rows` rows (at least 1) wherever the steps allow (see
    /// [`Options::with_band`](crate::Options::with_band)).
    ///
    /// A maximal run of steps that each have a margin is made band by band. The window of a band is the rows of the
    /// input from the sum of the margins above its first row to the same below its last (fewer where the picture
    /// ends); every step of the run is given the whole window and its place on the canvas, so each step reads real rows
    /// wherever the rows it keeps are concerned, and the rows beyond the band, which the margins consumed, are
    /// dropped. The rows of every band are copied into one output image. A step without a margin runs whole.
    pub(super) fn advance_banded(&mut self, rows: usize) -> Result<(), Error> {
        let steps: Vec<Prepared> = self.steps.by_ref().collect();
        let (height, width) = {
            let picture = self.working.picture();
            (picture.height(), picture.width())
        };
        let canvas = Canvas::new(height, width);
        let margins = steps
            .iter()
            .map(|step| step.margin(&canvas))
            .collect::<Result<Vec<_>, _>>()?;
        let mut first = 0;
        while first < steps.len() {
            let Some(_) = margins[first] else {
                self.whole(&steps[first])?;
                first += 1;
                continue;
            };
            let end = first + margins[first..].iter().take_while(|margin| margin.is_some()).count();
            let total: usize = margins[first..end].iter().flatten().sum();
            if rows.saturating_add(total) >= height {
                // a band and its margin reach the picture: banding would save nothing
                for step in &steps[first..end] {
                    self.whole(step)?;
                }
            } else {
                self.band_run(&steps[first..end], rows, total)?;
            }
            first = end;
        }
        Ok(())
    }

    /// `step` run on the whole picture, as an unbanded render does.
    fn whole(&mut self, step: &Prepared) -> Result<(), Error> {
        let before = self
            .current
            .as_ref()
            .map_or_else(|| self.working.picture(), Image::as_picture);
        let after = step.apply(&before, self.ratio, &mut self.regions, &self.working.picture(), 0)?;
        self.regions.release(self.ran);
        self.ran += 1;
        self.current = Some(after);
        Ok(())
    }

    /// `steps`, all with a margin whose sum is `margin`, run in bands of `rows` rows.
    fn band_run(&mut self, steps: &[Prepared], rows: usize, margin: usize) -> Result<(), Error> {
        let working = self.working.picture();
        let (height, width) = (working.height(), working.width());
        let mut out = vec![[0.0_f32; 3]; height * width];
        {
            let input = self
                .current
                .as_ref()
                .map_or_else(|| self.working.picture(), Image::as_picture);
            for top in (0..height).step_by(rows) {
                let bottom = (top + rows).min(height);
                let (from, to) = (top.saturating_sub(margin), (bottom + margin).min(height));
                let mut image: Option<Image> = None;
                for step in steps {
                    let before: Picture<'_> = image.as_ref().map_or_else(|| input.rows(from, to), Image::as_picture);
                    let after = step.apply(&before, self.ratio, &mut self.regions, &working, from)?;
                    image = Some(after);
                }
                let made = image.as_ref().map(Image::as_picture);
                if let Some(made) = made {
                    out[top * width..bottom * width]
                        .copy_from_slice(&made.pixels()[(top - from) * width..(bottom - from) * width]);
                }
            }
        }
        for _ in steps {
            self.regions.release(self.ran);
            self.ran += 1;
        }
        self.current = Some(Image::from_pixels(height, width, out));
        Ok(())
    }
}
