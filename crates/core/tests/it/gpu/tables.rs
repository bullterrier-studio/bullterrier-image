//! The tables a kernel reads cross to the device counted, whatever kernel sends them, and one the device cannot bind is
//! refused naming its size.

use std::sync::Arc;

use bullterrier_image::{Error, ErrorKind, Lut, Options, Photo, Picture, Recipe, Step};
use pollster::block_on;

use super::{fresh, gpu, rendered};
use crate::common::pictures::varied_seeded;

#[test]
fn the_table_of_dead_pixels_is_counted_a_float_for_every_cell() -> crate::common::Outcome {
    let Some(gpu) = fresh() else { return Ok(()) };
    let values = varied_seeded(30, 40, 2);
    let photo = Photo::new(Picture::new(30, 40, &values)?);
    // cells of one pixel: a float for each of the 1,200
    let recipe = Recipe::new([Step::new("dead_pixels").param("size", 1)]);
    rendered(&gpu, &photo, &recipe, Options::default())?;
    let moved = gpu.transfers();
    assert_eq!((moved.uploads, moved.tables, moved.downloads), (1, 1, 1));
    assert_eq!(moved.bytes, 1200 * 12 + 1200 * 4 + 1200 * 16);
    Ok(())
}

#[test]
fn a_table_the_device_cannot_bind_is_a_gpu_error_naming_its_size() -> crate::common::Outcome {
    // held while the test makes a context of its own (`gpu::ONE_AT_A_TIME`)
    let Some(_alone) = gpu() else { return Ok(()) };
    // a caller's device whose storage buffers hold 64 KiB
    let (device, queue) = block_on(async {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await?;
        let descriptor = wgpu::DeviceDescriptor {
            required_limits: wgpu::Limits {
                max_storage_buffer_binding_size: 64 * 1024,
                ..wgpu::Limits::downlevel_defaults()
            },
            ..Default::default()
        };
        Ok::<_, Box<dyn std::error::Error>>(adapter.request_device(&descriptor).await?)
    })?;
    let small = bullterrier_image::Gpu::new(device, queue)?;
    let values = varied_seeded(8, 8, 1);
    let render = |size: usize| -> Result<(), Error> {
        let entries = vec![[0.5; 3]; size.pow(3)];
        let lut = Lut::new(size, [0.0; 3], [1.0; 3], entries)?;
        let photo = Photo::new(Picture::new(8, 8, &values)?).with_lut("grade", Arc::new(lut));
        let recipe = Recipe::new([Step::new("lut").param("file", "grade")]);
        rendered(&small, &photo, &recipe, Options::default()).map(drop)
    };
    // 17 a side is 58,956 bytes: held
    render(17)?;
    // 20 a side is 96,000 bytes: over
    let error = render(20).err().ok_or("over the device")?;
    assert_eq!(error.kind(), ErrorKind::Gpu, "{error}");
    let said = error.to_string();
    assert!(said.contains("96000 bytes") && said.contains("65536"), "{said}");
    Ok(())
}
