//! A picture sent to the device and read back is the picture that was sent, at sizes that are not multiples of a
//! workgroup.

use bullterrier_image::{Image, Picture};

use pollster::block_on;

use super::gpu;
use crate::common::compare::first_bit_difference;
use crate::common::pictures::varied_seeded;

const SIZES: [(usize, usize); 3] = [(1, 1), (7, 5), (301, 199)];

#[test]
fn a_picture_comes_back_as_it_was_sent() -> crate::common::Outcome {
    let Some(gpu) = gpu() else { return Ok(()) };
    for (index, (height, width)) in SIZES.into_iter().enumerate() {
        let values = varied_seeded(height, width, index);
        let sent = block_on(gpu.upload(&Picture::new(height, width, &values)?))?;
        assert_eq!((sent.height(), sent.width(), sent.channels()), (height, width, 3));
        let back = block_on(gpu.download(&sent))?;
        assert_eq!(back.channels(), 3);
        let back = back.into_values();
        assert_eq!(
            first_bit_difference(&back, &values),
            None,
            "{height} x {width} on {}",
            gpu.adapter()
        );
    }
    Ok(())
}

#[test]
fn an_image_with_alpha_comes_back_with_its_alpha() -> crate::common::Outcome {
    let Some(gpu) = gpu() else { return Ok(()) };
    for (index, (height, width)) in SIZES.into_iter().enumerate() {
        let values: Vec<f32> = varied_seeded(height, width * 4, index + 7)[..height * width * 4].to_vec();
        let image = Image::rgba(height, width, &values)?;
        let sent = block_on(gpu.upload_image(&image))?;
        assert_eq!(sent.channels(), 4);
        let back = block_on(gpu.download(&sent))?;
        assert_eq!(back.channels(), 4);
        assert_eq!(
            first_bit_difference(&back.into_values(), &values),
            None,
            "{height} x {width}"
        );
    }
    Ok(())
}

#[test]
fn a_buffer_of_the_callers_device_is_a_picture_and_a_picture_is_a_buffer() -> crate::common::Outcome {
    let Some(_alone) = gpu() else { return Ok(()) };
    let (device, queue) = block_on(async {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await?;
        Ok::<_, Box<dyn std::error::Error>>(adapter.request_device(&wgpu::DeviceDescriptor::default()).await?)
    })?;
    let gpu = bullterrier_image::Gpu::new(device.clone(), queue.clone())?;
    // 2 x 3 pixels the caller writes: straight rgba, alpha 0.25 on the second
    let values: Vec<f32> = (0..24u8).map(|at| f32::from(at) / 24.0).collect();
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 6 * 16,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(
        &buffer,
        0,
        &values.iter().flat_map(|value| value.to_le_bytes()).collect::<Vec<u8>>(),
    );
    let adopted = gpu.adopt(buffer, 2, 3, true)?;
    assert_eq!((adopted.height(), adopted.width(), adopted.channels()), (2, 3, 4));
    assert_eq!(adopted.buffer().size(), 6 * 16);
    let back = block_on(gpu.download(&adopted))?;
    assert_eq!(back.channels(), 4);
    assert_eq!(first_bit_difference(&back.into_values(), &values), None);
    assert_eq!(gpu.transfers().uploads, 0, "adopting sends nothing");
    // too small a buffer is refused
    let small = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 16,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    assert!(gpu.adopt(small, 2, 3, true).is_err());
    Ok(())
}
