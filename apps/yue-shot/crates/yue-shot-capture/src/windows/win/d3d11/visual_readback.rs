//! P2 isolated transfer measurements. No capture window, hotkey or clipboard.
use super::*;
use serde_json::json;
use std::time::Instant;

#[test]
#[ignore = "P2 opt-in real GPU transfer cost; allocates test-only 4K/8K full frames"]
fn visual_readback_full_roi_and_strips_have_exact_pixels() {
    let device = GraphicsDevice::create().expect("real hardware D3D11 device required");
    let mut rows = Vec::new();
    for (width, height) in [(3840u32, 2160u32), (7680, 4320)] {
        let mut source = vec![0u8; width as usize * height as usize * 4];
        for (i, pixel) in source.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let x = i % width as usize;
            let y = i / width as usize;
            pixel.copy_from_slice(&[(x % 251) as u8, (y % 239) as u8, ((x + y) % 233) as u8, 255]);
        }
        let source_hash = blake3::hash(&source);
        let texture = device.create_bgra_texture(width, height, &source).unwrap();
        let x = width / 2 - 256;
        let y = height / 2 - 256;
        for (name, regions) in [
            ("full", vec![[0, 0, width, height]]),
            ("roi_512", vec![[x, y, 512, 512]]),
            (
                "cross_strips",
                vec![
                    [0, height / 2 - 12, width, 25],
                    [width / 2 - 12, 0, 25, height],
                ],
            ),
            (
                "four_edges",
                vec![
                    [12, 12, width - 24, 25],
                    [12, height - 37, width - 24, 25],
                    [12, 37, 25, height - 74],
                    [width - 37, 37, 25, height - 74],
                ],
            ),
        ] {
            let mut timings = Vec::new();
            let mut first_us = 0;
            for iteration in 0..106 {
                let started = Instant::now();
                // The API includes allocation, staging creation, Copy, blocking
                // Map and tight row copy. All context calls stay on this owner.
                let results: Vec<_> = regions
                    .iter()
                    .map(|&[rx, ry, rw, rh]| {
                        device
                            .read_back_region_bgra(&texture.texture, rx, ry, rw, rh)
                            .unwrap()
                    })
                    .collect();
                let elapsed_us = started.elapsed().as_micros();
                if iteration == 0 {
                    first_us = elapsed_us;
                }
                if iteration >= 6 {
                    timings.push(elapsed_us);
                }
                for (region, actual) in regions.iter().zip(&results) {
                    let [rx, ry, rw, rh] = region.map(|v| v as usize);
                    assert_eq!(actual.len(), rw * rh * 4);
                    for row in 0..rh {
                        let offset = ((ry + row) * width as usize + rx) * 4;
                        assert_eq!(
                            &actual[row * rw * 4..(row + 1) * rw * 4],
                            &source[offset..offset + rw * 4]
                        );
                    }
                }
            }
            assert_eq!(timings.len(), 100);
            let bytes: u64 = regions
                .iter()
                .map(|r| u64::from(r[2]) * u64::from(r[3]) * 4)
                .sum();
            rows.push(json!({"size":[width,height],"operation":name,"regions":regions,"calls_per_query":regions.len(),"tight_bytes":bytes,"first_use_us":first_us,"warm_samples_us":timings}));
        }
        assert_eq!(source_hash, blake3::hash(&source));
        assert_eq!(device.read_back_bgra(&texture.texture).unwrap(), source);
    }
    println!(
        "VISUAL_READBACK {}",
        json!({"scope":"isolated_idle_SDR_hardware_only; not UI/render/end-to-end qualification","warmup":5,"first_use_separate":true,"iterations":100,"rows":rows,"assertions":["exact_all_rows_every_sample","source_unchanged","single_context_owner"],"not_measured":["GPU_busy","presentation_contention","HDR","BitBlt","staging_reuse","worker_queue"]})
    );
}

#[test]
#[ignore = "P2 isolated real GPU to owned CPU worker prototype; no overlay, hotkey or clipboard"]
fn visual_readback_cpu_worker_region_pipeline_reference() {
    use crate::geometry::{Point, Rect};
    use crate::visual_detection::{Image, proposals};
    let device = GraphicsDevice::create().unwrap();
    let mut rows = Vec::new();
    for (name, region, point) in [
        ("T01.png", [0, 0, 400, 200], Point::new(91, 66)),
        ("T10.png", [1664, 824, 512, 512], Point::new(1816, 1066)),
        ("T21.png", [288, 218, 304, 184], Point::new(410, 302)),
    ] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/visual-candidate-v4.1")
            .join(name);
        let mut decoder = png::Decoder::new(std::io::Cursor::new(std::fs::read(path).unwrap()));
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::ALPHA);
        let mut reader = decoder.read_info().unwrap();
        let mut rgba = vec![0u8; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut rgba).unwrap();
        assert_eq!(info.color_type, png::ColorType::Rgba);
        rgba.truncate(info.buffer_size());
        let size = [info.width as usize, info.height as usize];
        let [x, y, w, h] = region;
        let coverage = Rect::new(x as i32, y as i32, (x + w) as i32, (y + h) as i32);
        let reference = proposals::seed(
            Image::new(size, &rgba)
                .unwrap()
                .with_coverage(coverage)
                .unwrap(),
            point,
        );
        let expected = serde_json::to_value(&reference).unwrap();
        assert!(
            !reference.candidates.is_empty(),
            "worker prototype needs a positive target"
        );
        let mut bgra = rgba.clone();
        for p in bgra.as_chunks_mut::<4>().0 {
            p.swap(0, 2);
        }
        let texture = device
            .create_bgra_texture(info.width, info.height, &bgra)
            .unwrap();
        let (jobs, receive) = std::sync::mpsc::sync_channel::<Vec<u8>>(1);
        let (reply, results) = std::sync::mpsc::sync_channel(1);
        let local_point = Point::new(point.x - x as i32, point.y - y as i32);
        let worker = std::thread::spawn(move || {
            for mut bytes in receive {
                let started = Instant::now();
                for p in bytes.as_chunks_mut::<4>().0 {
                    p.swap(0, 2);
                }
                let detection = proposals::seed(
                    Image::new([w as usize, h as usize], &bytes).unwrap(),
                    local_point,
                );
                if reply
                    .send((detection, started.elapsed().as_micros()))
                    .is_err()
                {
                    break;
                }
            }
        });
        let mut total = Vec::new();
        let mut readback = Vec::new();
        let mut compute = Vec::new();
        for iteration in 0..106 {
            let started = Instant::now();
            let bytes = device
                .read_back_region_bgra(&texture.texture, x, y, w, h)
                .unwrap();
            let owner_us = started.elapsed().as_micros();
            jobs.send(bytes).unwrap();
            let (output, worker_us) = results
                .recv_timeout(std::time::Duration::from_secs(10))
                .unwrap();
            let elapsed_us = started.elapsed().as_micros();
            let mut actual = serde_json::to_value(&output).unwrap();
            for candidate in actual["candidates"].as_array_mut().unwrap() {
                let rect = candidate["rect"].as_array_mut().unwrap();
                rect[0] = json!(rect[0].as_i64().unwrap() + i64::from(x));
                rect[1] = json!(rect[1].as_i64().unwrap() + i64::from(y));
            }
            let geometry = |v: &serde_json::Value| {
                v["candidates"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|c| c["rect"].clone())
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                geometry(&actual),
                geometry(&expected),
                "region/worker origin or channel mismatch"
            );
            assert_eq!(actual["completion"], expected["completion"]);
            if iteration >= 6 {
                total.push(elapsed_us);
                readback.push(owner_us);
                compute.push(worker_us);
            }
        }
        drop(jobs);
        worker.join().unwrap();
        assert_eq!(
            device.read_back_bgra(&texture.texture).unwrap(),
            bgra,
            "analysis modified the GPU source"
        );
        rows.push(json!({"fixture":name,"source_size":size,"region":region,"read_bytes":w*h*4,"warm_end_to_end_us":total,"warm_owner_us":readback,"warm_worker_us":compute,"matches":106,"source_unchanged":true}));
    }
    println!(
        "VISUAL_WORKER {}",
        json!({"scope":"isolated synchronous owner readback + single owned-buffer CPU worker; ROI coverage equivalence only","rows":rows,"not_measured":["whole-frame hierarchy outside ROI","production cancellation/source gate","UI presentation/contention","HDR"],"max_inflight_jobs":1,"channel_capacity":1})
    );
}

#[test]
#[ignore = "P2 real demand transfers; intentionally measures unbatched small-Map cost"]
fn visual_readback_bounded_demand_keeps_whole_frame_hierarchy() {
    use crate::geometry::{Point, Rect};
    use crate::visual_detection::{Completion, Image, proposals, reads};
    use std::cell::{Cell, RefCell};
    let device = GraphicsDevice::create().unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut rows = Vec::new();
    for (directory, name, point) in [
        ("visual-candidate-v4.1", "T21.png", Point::new(410, 302)),
        ("visual-candidate-v4.1", "T10.png", Point::new(1816, 1066)),
        (
            "visual-candidate-8k-v4",
            "T10-8k.png",
            Point::new(1816, 1066),
        ),
    ] {
        let mut decoder = png::Decoder::new(std::io::Cursor::new(
            std::fs::read(root.join(directory).join(name)).unwrap(),
        ));
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::ALPHA);
        let mut reader = decoder.read_info().unwrap();
        let mut rgba = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut rgba).unwrap();
        assert_eq!(info.color_type, png::ColorType::Rgba);
        rgba.truncate(info.buffer_size());
        let size = [info.width as usize, info.height as usize];
        // RGB/BGR permutation leaves every max-channel norm invariant. This
        // prototype deliberately keeps uploaded BGRA bytes and uses that oracle.
        let mut bgra = rgba.clone();
        for p in bgra.as_chunks_mut::<4>().0 {
            p.swap(0, 2);
        }
        let expected = proposals::seed(Image::new(size, &bgra).unwrap(), point);
        assert_eq!(expected.completion, Completion::Complete);
        assert!(!expected.candidates.is_empty());
        let texture = device
            .create_bgra_texture(info.width, info.height, &bgra)
            .unwrap();
        let owner_us = Cell::new(0u128);
        let max_call_us = Cell::new(0u128);
        let started = Instant::now();
        let pixels = RefCell::new(
            reads::DemandPixels::new(size, point, |r: Rect| {
                let transfer_start = Instant::now();
                let data = device
                    .read_back_region_bgra(
                        &texture.texture,
                        r.left as u32,
                        r.top as u32,
                        r.width() as u32,
                        r.height() as u32,
                    )
                    .map_err(|_| Completion::Failed)?;
                let elapsed = transfer_start.elapsed().as_micros();
                owner_us.set(owner_us.get() + elapsed);
                max_call_us.set(max_call_us.get().max(elapsed));
                // Independent row comparison for every requested block, including
                // initial ROI; no full-frame read is used to service the algorithm.
                for row in 0..r.height() as usize {
                    let start = ((r.top as usize + row) * size[0] + r.left as usize) * 4;
                    assert_eq!(
                        &data[row * r.width() as usize * 4..(row + 1) * r.width() as usize * 4],
                        &bgra[start..start + r.width() as usize * 4]
                    );
                }
                Ok(data)
            })
            .unwrap(),
        );
        let fetch = |p| pixels.borrow_mut().pixel(p);
        let actual = proposals::seed(Image::from_reader(size, &fetch).unwrap(), point);
        let elapsed_us = started.elapsed().as_micros();
        let pixels = pixels.borrow();
        assert_eq!(pixels.failure, None);
        assert_eq!(
            serde_json::to_value(&actual).unwrap(),
            serde_json::to_value(&expected).unwrap()
        );
        assert!(pixels.extra_bytes <= 1_048_576);
        assert_eq!(device.read_back_bgra(&texture.texture).unwrap(), bgra);
        rows.push(json!({"fixture":name,"size":size,"point":[point.x,point.y],"end_to_end_us":elapsed_us,"owner_total_us":owner_us.get(),"max_call_us":max_call_us.get(),"calls":pixels.calls,"initial_bytes":pixels.initial_bytes(),"extra_bytes":pixels.extra_bytes,"source_unchanged":true,"whole_frame_equivalent":true,"candidates":actual.candidates.len()}));
    }
    println!(
        "VISUAL_BOUNDED_READBACK {}",
        json!({"scope":"single-cold-query synchronous demand-transfer oracle; not performance qualification or CPU-worker product path","rows":rows})
    );
}
