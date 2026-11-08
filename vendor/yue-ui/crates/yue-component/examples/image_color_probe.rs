//! Read-only PNG header / ICC probe. Does not decode a full photograph or open a window.
use moxcms::{ColorProfile, Layout, ToneReprCurve, TransformOptions};
use std::{fs::File, io::BufReader};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    for path in std::env::args().skip(1) {
        let decoder = png::Decoder::new(BufReader::new(File::open(&path)?));
        let reader = decoder.read_info()?;
        let info = reader.info();
        println!(
            "file={path} size={}x{} depth={:?} cicp={:?}",
            info.width, info.height, info.bit_depth, info.coding_independent_code_points
        );
        if let Some(bytes) = &info.icc_profile {
            let profile = ColorProfile::new_from_slice(bytes)?;
            println!(
                "icc_bytes={} cicp={:?} luminance={:?} class={:?} pcs={:?}",
                bytes.len(),
                profile.cicp,
                profile.luminance,
                profile.profile_class,
                profile.pcs
            );
            println!("description={:?}", profile.description);
            let mut linear = ColorProfile::new_srgb();
            linear.red_trc = Some(ToneReprCurve::Parametric(vec![1.0]));
            linear.green_trc = linear.red_trc.clone();
            linear.blue_trc = linear.red_trc.clone();
            linear.cicp = None;
            for prefer_fixed_point in [true, false] {
                let transform = profile.create_transform_f32(
                    Layout::Rgb,
                    &linear,
                    Layout::Rgb,
                    TransformOptions {
                        prefer_fixed_point,
                        allow_extended_range_rgb_xyz: true,
                        ..TransformOptions::default()
                    },
                )?;
                for code in [0.0, 0.25, 0.5, 0.75, 1.0] {
                    let mut output = [0.0; 3];
                    transform.transform(&[code; 3], &mut output)?;
                    println!(
                        "icc fixed={prefer_fixed_point} code={code} relative_linear={output:?}"
                    );
                }
            }
        }
    }
    Ok(())
}
