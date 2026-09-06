//! Native Forma offscreen scenarios. Golden readback/serialization are outside measurement.
use rbench::{image::RgbaImage, *};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::PathBuf, time::Instant};
#[global_allocator]
static ALLOC: alloc::TrackingAllocator<std::alloc::System> =
    alloc::TrackingAllocator::new(std::alloc::System);
const N: usize = 96;
const SCENARIOS: [&str; 7] = [
    "static",
    "hover",
    "animation",
    "scroll",
    "resize",
    "text",
    "image",
];
const SCROLL:&str="component Demo { Frame { width:400; height:200; padding:10; clip:true; background:#181e2a; Scroll { Button { width:600; height:400; text:'Scroll content'; } } } }";
struct Options {
    filter: String,
    goldens: PathBuf,
    record: bool,
    tolerance: u8,
    percent: f64,
}
fn main() -> Result<()> {
    let mut o = Options {
        filter: String::new(),
        goldens: PathBuf::from(".rbench/forma-goldens"),
        record: false,
        tolerance: 0,
        percent: 0.,
    };
    let mut args = std::env::args().skip(1);
    let mut list = false;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--list" => list = true,
            "--json" => {}
            "--filter" => o.filter = args.next().ok_or_else(|| error("filter required"))?,
            "--goldens" => {
                o.goldens = args
                    .next()
                    .ok_or_else(|| error("golden directory required"))?
                    .into()
            }
            "--record-goldens" => {
                o.record = true;
                o.goldens = args
                    .next()
                    .ok_or_else(|| error("new golden directory required"))?
                    .into();
            }
            "--channel-tolerance" => {
                o.tolerance = args
                    .next()
                    .ok_or_else(|| error("tolerance required"))?
                    .parse()?
            }
            "--max-changed-percent" => {
                o.percent = args
                    .next()
                    .ok_or_else(|| error("percentage required"))?
                    .parse()?
            }
            "--help" => {
                println!("--list --filter TEXT --record-goldens NEW_DIR | --goldens DIR [--channel-tolerance 0..255 --max-changed-percent 0..100] --json\nGoldens are required. Recording creates reference images only; inspect PNGs before using them. image is unsupported by this native renderer.");
                return Ok(());
            }
            _ => return Err(error(format!("unknown argument {a}"))),
        }
    }
    if list {
        for s in SCENARIOS.into_iter().filter(|s| s.contains(&o.filter)) {
            println!(
                "forma/{s}{}",
                if s == "image" {
                    " [unsupported: native image primitive absent]"
                } else {
                    ""
                }
            );
        }
        return Ok(());
    }
    if !o.percent.is_finite() || !(0.0..=100.0).contains(&o.percent) {
        return Err(error("percentage must be 0..100"));
    }
    if SCENARIOS.iter().all(|s| !s.contains(&o.filter)) {
        return Err(error("no matching scenario"));
    }
    if o.record {
        if let Some(p) = o.goldens.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(p)?;
        }
        std::fs::create_dir(&o.goldens)?;
    }
    pollster::block_on(run(o))
}
fn source(name: &str) -> String {
    match name {
        "scroll" => SCROLL.into(),
        "text" => forma_vector::EXAMPLE.replace("Найти документы", "Текст ABC 123 — тест"),
        _ => forma_vector::EXAMPLE.into(),
    }
}
fn state(model: &mut forma_vector::Button, name: &str, frame: usize) -> (u32, u32) {
    match name {
        "hover" => {
            model.pointer(100., 90., 0);
            model.tick(1000.);
        }
        "animation" => {
            model.pointer(if (frame / 12) & 1 == 0 { 100. } else { -10. }, 90., 0);
            model.tick(16.);
        }
        "scroll" => {
            model.scroll(-10000., -10000.);
            model.scroll((frame % 40) as f32, (frame % 60) as f32);
        }
        _ => {}
    }
    if name == "resize" && frame % 2 == 1 {
        (640, 320)
    } else {
        (800, 400)
    }
}
struct Surface {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    w: u32,
    h: u32,
}
fn surface(r: &forma_vector::gpu::Renderer, w: u32, h: u32) -> Surface {
    let texture = r.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("rbench-golden-target"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    Surface {
        texture,
        view,
        w,
        h,
    }
}
fn draw(r: &mut forma_vector::gpu::Renderer, m: &forma_vector::Button, s: &Surface) -> Result<()> {
    r.draw(m, &s.view, s.w, s.h, 2., true, true)
        .map_err(error)?;
    Ok(())
}
fn pixels(r: &forma_vector::gpu::Renderer, s: &Surface) -> Result<RgbaImage> {
    let stride = (s.w * 4).div_ceil(256) * 256;
    let b = r.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("golden readback outside measurement"),
        size: u64::from(stride) * u64::from(s.h),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut e = r.device.create_command_encoder(&Default::default());
    e.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &s.texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &b,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(s.h),
            },
        },
        wgpu::Extent3d {
            width: s.w,
            height: s.h,
            depth_or_array_layers: 1,
        },
    );
    r.queue.submit(Some(e.finish()));
    let (send, recv) = std::sync::mpsc::channel();
    b.slice(..).map_async(wgpu::MapMode::Read, move |v| {
        let _ = send.send(v);
    });
    r.device.poll(wgpu::PollType::wait_indefinitely())?;
    recv.recv()??;
    let mapped = b.slice(..).get_mapped_range();
    let mut data = Vec::with_capacity((s.w * s.h * 4) as usize);
    for row in mapped.chunks_exact(stride as usize) {
        data.extend_from_slice(&row[..s.w as usize * 4]);
    }
    drop(mapped);
    b.unmap();
    Ok(RgbaImage {
        width: s.w,
        height: s.h,
        pixels: data,
    })
}
fn golden(
    o: &Options,
    name: &str,
    frame: usize,
    image: &RgbaImage,
    record: bool,
) -> Result<String> {
    let path = o.goldens.join(format!("{name}-{frame}.rbimg"));
    if record {
        image.save_new(&path)?;
        let f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path.with_extension("png"))?;
        let mut encoder = png::Encoder::new(f, image.width, image.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header()?.write_image_data(&image.pixels)?;
    } else {
        let expected = RgbaImage::load(&path).map_err(|e| {
            error(format!(
                "{}: {e}; record and inspect reference goldens first",
                path.display()
            ))
        })?;
        let difference = image.compare(&expected, o.tolerance, o.percent)?;
        if !difference.accepted {
            return Err(error(format!(
                "golden mismatch {name} frame {frame}: {}",
                serde_json::to_string(&difference)?
            )));
        }
    }
    model::hash_file(&path)
}
fn metrics() -> Vec<Metric> {
    let phase = |id: &str, scope: &str| Metric {
        id: id.into(),
        unit: "calls".into(),
        scope: scope.into(),
        phase: "measurement".into(),
        statistic: "96-frame phase total".into(),
        direction: Direction::Lower,
    };
    vec![
        Metric::duration(
            "cpu.submit",
            "scenario update + CPU encode and submit wall",
            "individual frame",
        ),
        Metric::duration(
            "frame.completed",
            "scenario update + CPU submit + GPU wait; no present",
            "individual frame",
        ),
        phase(
            "alloc.calls",
            "whole-process Rust allocator; native/driver excluded",
        ),
        phase("geometry.uploads", "Forma renderer geometry"),
    ]
}
async fn run(o: Options) -> Result<()> {
    let instance = wgpu::Instance::default();
    let adapter = instance.request_adapter(&Default::default()).await?;
    let mut recorder = Recorder::new();
    for name in SCENARIOS.into_iter().filter(|s| s.contains(&o.filter)) {
        let id = format!("forma/{name}");
        if name == "image" {
            if !o.record {
                recorder.case(Case {
                    id: id.clone(),
                    contract: BTreeMap::from([(
                        "scenario".into(),
                        "native image primitive".into(),
                    )]),
                    metrics: metrics(),
                })?;
                for metric in metrics() {
                    recorder.unavailable(&id,&metric.id,Availability::Unsupported("This Forma native snapshot has no Image display-list/GPU primitive; JS image component is not a native image workload".into()))?;
                }
            }
            continue;
        }
        eprintln!(
            "rbench: {id} — {}",
            if o.record {
                "recording reference checkpoints"
            } else {
                "golden pre-validation"
            }
        );
        let src = source(name);
        let mut hashes = BTreeMap::new();
        // Separate correctness pass, then a fresh measured instance with identical warmup/state sequence.
        for pass in 0..if o.record { 1 } else { 2 } {
            let mut model =
                forma_vector::Button::from_sources(&src, forma_vector::BUTTON_COMPONENT)
                    .map_err(error)?;
            let mut renderer =
                forma_vector::gpu::Renderer::new(&adapter, wgpu::TextureFormat::Rgba8Unorm)
                    .await
                    .map_err(error)?;
            let large = surface(&renderer, 800, 400);
            let small = surface(&renderer, 640, 320);
            for _ in 0..20 {
                draw(&mut renderer, &model, &large)?;
                renderer.device.poll(wgpu::PollType::wait_indefinitely())?;
            }
            let mut submits = [0u128; N];
            let mut completes = [0u128; N];
            let uploads = renderer.uploads;
            if pass == 1 {
                eprintln!("rbench: {id} — measuring {N} frames");
            }
            let before = ALLOC.snapshot();
            for i in 0..N {
                let start = Instant::now();
                let (w, _) = state(&mut model, name, i);
                let s = if w == 800 { &large } else { &small };
                draw(&mut renderer, &model, s)?;
                submits[i] = start.elapsed().as_nanos();
                renderer.device.poll(wgpu::PollType::wait_indefinitely())?;
                completes[i] = start.elapsed().as_nanos();
                if pass == 0 && (i == 0 || i == N - 1) {
                    let hash = golden(&o, name, i, &pixels(&renderer, s)?, o.record)?;
                    hashes.insert(format!("golden.{i}"), hash);
                }
            }
            let after = ALLOC.snapshot();
            if pass == 1 {
                let final_surface = if name == "resize" { &small } else { &large };
                let final_hash =
                    golden(&o, name, N - 1, &pixels(&renderer, final_surface)?, false)?;
                if hashes.get(&format!("golden.{}", N - 1)) != Some(&final_hash) {
                    return Err(error("golden changed during measurement"));
                }
                if name == "static" && renderer.uploads != uploads {
                    return Err(error("static geometry unexpectedly reuploaded"));
                }
                let mut contract = BTreeMap::from([
                    ("adapter".into(), format!("{:?}", adapter.get_info())),
                    (
                        "fixture.sha256".into(),
                        format!(
                            "{:x}",
                            Sha256::digest(format!("{src}\n{}", forma_vector::BUTTON_COMPONENT))
                        ),
                    ),
                    ("scenario".into(), name.into()),
                    ("scenario.version".into(), "1".into()),
                    (
                        "viewport".into(),
                        if name == "resize" {
                            "alternating 800x400/640x320; preallocated targets"
                        } else {
                            "800x400"
                        }
                        .into(),
                    ),
                    ("dpi".into(), "2".into()),
                    ("frames".into(), N.to_string()),
                    ("warmup".into(), "20 static frames".into()),
                    (
                        "completion".into(),
                        "serialized offscreen wait; no present".into(),
                    ),
                    (
                        "validation".into(),
                        format!(
                            "pre checkpoints 0,95 and post 95; RGBA8 tolerance {} / {}%",
                            o.tolerance, o.percent
                        ),
                    ),
                    (
                        "instrumentation".into(),
                        "atomic Rust allocator; recording/readback outside measured phase".into(),
                    ),
                ]);
                contract.extend(hashes.clone());
                recorder.case(Case {
                    id: id.clone(),
                    contract,
                    metrics: metrics(),
                })?;
                for (s, c) in submits.into_iter().zip(completes) {
                    recorder.observe(&id, "cpu.submit", s)?;
                    recorder.observe(&id, "frame.completed", c)?;
                }
                recorder.observe(
                    &id,
                    "alloc.calls",
                    u128::from(after.allocations - before.allocations),
                )?;
                recorder.observe(
                    &id,
                    "geometry.uploads",
                    u128::from(renderer.uploads - uploads),
                )?;
            }
        }
    }
    if o.record {
        model::write_new(
            &o.goldens.join("reference.json"),
            &serde_json::json!({"adapter":format!("{:?}",adapter.get_info()),"filter":o.filter,"frames":N,"checkpoints":[0,95],"note":"Captured references; inspect PNGs. This is not an independent correctness oracle."}),
        )?;
        println!(
            "Recorded {}. Inspect PNGs before benchmarking.",
            o.goldens.display()
        );
    } else {
        recorder.note("Six native offscreen scenarios; checkpoint golden equality is not full-frame-sequence equivalence or window FPS. Resize switches preallocated targets; texture allocation/window resize excluded. Image scenario is explicitly unsupported. Golden readback and recorder serialization are outside measurement.");
        println!(
            "RBENCH_RESULT={}",
            serde_json::to_string(&recorder.finish()?)?
        );
    }
    Ok(())
}
