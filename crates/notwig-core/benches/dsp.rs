use std::collections::BTreeMap;
use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use notwig_core::dsp::stock::{
    ChainProcessor, EqProcessor, ToolProcessor, band_param, chain_spec, eq_magnitude_response,
    eq_spec, p_dry_wet, p_eq_output_gain, p_eq_shift, p_input_gain, p_output_gain, p_pan,
    p_polarity_l, p_polarity_r, p_width, tool_spec,
};
use notwig_core::dsp::{
    DeviceProcessor, DeviceRegistry, DeviceSpec, EngineCaps, ProcessContext, RenderConfig,
    TransportInfo, compile, stock_registry,
};
use notwig_core::ids::{Device, DeviceChain, Track};
use notwig_core::protocol::{Command, CommandPayload};
use notwig_core::{
    DeviceKind, Id, Meta, ParamValue, ProjectDoc, SampleRate, SchemaVersion, TrackKind, apply,
};
use uuid::Uuid;

const SAMPLE_RATE: u32 = 48_000;
const BLOCK_SIZES: [usize; 3] = [64, 256, 1024];

fn id<T>(n: u128) -> Id<T> {
    Id::from_uuid(Uuid::from_u128(n))
}

fn config(max_block_size: usize) -> RenderConfig {
    RenderConfig {
        sample_rate: SAMPLE_RATE,
        max_block_size,
        channels: 2,
        tempo_bpm: 120.0,
    }
}

fn context(config: RenderConfig) -> ProcessContext<'static> {
    static TRANSPORT: TransportInfo = TransportInfo {
        playhead_mus: notwig_core::TimeMus::zero(),
        loop_start_mus: None,
        loop_end_mus: None,
    };
    ProcessContext {
        config,
        sample_position: 0,
        playing: true,
        tempo_bpm: 120.0,
        transport: &TRANSPORT,
    }
}

fn tool_processor(spec: &DeviceSpec) -> ToolProcessor {
    let mut tool = ToolProcessor::new(spec);
    tool.prepare(config(1024)).unwrap();
    tool.set_param(p_input_gain(), &ParamValue::F64(3.0));
    tool.set_param(p_output_gain(), &ParamValue::F64(-1.0));
    tool.set_param(p_pan(), &ParamValue::F64(0.25));
    tool.set_param(p_width(), &ParamValue::F64(1.2));
    tool.set_param(p_dry_wet(), &ParamValue::F64(0.8));
    tool.set_param(p_polarity_l(), &ParamValue::Bool(false));
    tool.set_param(p_polarity_r(), &ParamValue::Bool(true));
    tool
}

fn eq_processor(spec: &DeviceSpec) -> EqProcessor {
    let mut eq = EqProcessor::new(spec);
    eq.prepare(config(1024)).unwrap();
    let bands = [
        (120.0, 3.0, 0.7),
        (800.0, -2.0, 1.0),
        (3_000.0, 2.5, 1.4),
        (9_000.0, -4.0, 0.9),
    ];
    for (band, (freq, gain, q)) in bands.into_iter().enumerate() {
        eq.set_param(band_param(band, 0), &ParamValue::Bool(true));
        eq.set_param(band_param(band, 1), &ParamValue::EnumIndex(0));
        eq.set_param(band_param(band, 2), &ParamValue::F64(freq));
        eq.set_param(band_param(band, 3), &ParamValue::F64(gain));
        eq.set_param(band_param(band, 4), &ParamValue::F64(q));
    }
    eq.set_param(p_eq_output_gain(), &ParamValue::F64(-1.0));
    eq
}

fn chain_processor(spec: &DeviceSpec) -> ChainProcessor {
    let mut chain = ChainProcessor::new(spec);
    chain.prepare(config(1024)).unwrap();
    let tool_spec = tool_spec();
    let eq_spec = eq_spec();
    chain.add_child(id::<Device>(1), Box::new(tool_processor(&tool_spec)));
    chain.add_child(id::<Device>(2), Box::new(eq_processor(&eq_spec)));
    chain.add_child(id::<Device>(3), Box::new(tool_processor(&tool_spec)));
    chain
}

fn processed_audio(frames: usize) -> Vec<f32> {
    (0..frames * 2)
        .map(|index| ((index as f32) * 0.01).sin() * 0.25)
        .collect()
}

fn bench_tool(c: &mut Criterion) {
    let spec = tool_spec();
    let ctx = context(config(BLOCK_SIZES[2]));
    let mut group = c.benchmark_group("stock/tool/process");
    for frames in BLOCK_SIZES {
        group.throughput(Throughput::Elements(frames as u64 * 2));
        group.bench_with_input(
            BenchmarkId::from_parameter(frames),
            &frames,
            |b, &frames| {
                let mut tool = tool_processor(&spec);
                let mut audio = processed_audio(frames);
                b.iter(|| {
                    tool.process(black_box(&ctx), black_box(&mut audio), black_box(&[]));
                });
            },
        );
    }
    group.finish();
}

fn bench_eq(c: &mut Criterion) {
    let spec = eq_spec();
    let ctx = context(config(BLOCK_SIZES[2]));
    let mut group = c.benchmark_group("stock/eq/process");
    for frames in BLOCK_SIZES {
        group.throughput(Throughput::Elements(frames as u64 * 2));
        group.bench_with_input(
            BenchmarkId::from_parameter(frames),
            &frames,
            |b, &frames| {
                let mut eq = eq_processor(&spec);
                let mut audio = processed_audio(frames);
                b.iter(|| {
                    eq.process(black_box(&ctx), black_box(&mut audio), black_box(&[]));
                });
            },
        );
    }
    group.finish();
}

fn bench_eq_disabled(c: &mut Criterion) {
    let spec = eq_spec();
    let ctx = context(config(BLOCK_SIZES[2]));
    let frames = 256;
    let mut group = c.benchmark_group("stock/eq/process-disabled");
    group.throughput(Throughput::Elements(frames as u64 * 2));
    group.bench_function("256", |b| {
        let mut eq = EqProcessor::new(&spec);
        eq.prepare(config(1024)).unwrap();
        let mut audio = processed_audio(frames);
        b.iter(|| {
            eq.process(black_box(&ctx), black_box(&mut audio), black_box(&[]));
        });
    });
    group.finish();
}

fn bench_chain(c: &mut Criterion) {
    let spec = chain_spec();
    let ctx = context(config(BLOCK_SIZES[2]));
    let mut group = c.benchmark_group("stock/chain/process");
    for frames in BLOCK_SIZES {
        group.throughput(Throughput::Elements(frames as u64 * 2));
        group.bench_with_input(
            BenchmarkId::from_parameter(frames),
            &frames,
            |b, &frames| {
                let mut chain = chain_processor(&spec);
                let mut audio = processed_audio(frames);
                b.iter(|| {
                    chain.process(black_box(&ctx), black_box(&mut audio), black_box(&[]));
                });
            },
        );
    }
    group.finish();
}

fn bench_eq_response(c: &mut Criterion) {
    let spec = eq_spec();
    let mut values = BTreeMap::new();
    for param in &spec.params {
        values.insert(param.id, param.default.clone());
    }
    for band in 0..4 {
        values.insert(band_param(band, 0), ParamValue::Bool(true));
        values.insert(
            band_param(band, 2),
            ParamValue::F64(1_000.0 * (band + 1) as f64),
        );
        values.insert(band_param(band, 3), ParamValue::F64(3.0));
        values.insert(band_param(band, 4), ParamValue::F64(0.9));
    }
    values.insert(p_eq_shift(), ParamValue::F64(0.0));

    let frequencies: Vec<f64> = (0..256)
        .map(|index| 20.0 * (1_000.0_f64).powf(f64::from(index) / 255.0))
        .collect();
    let mut group = c.benchmark_group("graph/eq_magnitude_response");
    group.throughput(Throughput::Elements(frequencies.len() as u64));
    group.bench_function("256_points", |b| {
        b.iter(|| {
            let mut sum = 0.0;
            for frequency in &frequencies {
                sum +=
                    eq_magnitude_response(black_box(&values), f64::from(SAMPLE_RATE), *frequency);
            }
            black_box(sum);
        });
    });
    group.finish();
}

fn graph_doc(track_count: u128) -> ProjectDoc {
    let meta = Meta {
        name: "DSP benchmark".into(),
        created_utc: 0,
        modified_utc: 0,
        sample_rate: SampleRate::DEFAULT,
        tempo_bpm: 120.0,
        time_sig_num: 4,
        time_sig_den: 4,
    };
    let mut doc = ProjectDoc::new_template(meta, id::<Track>(1), id::<DeviceChain>(100)).unwrap();
    for index in 0..track_count {
        let track = id::<Track>(1_000 + index);
        let chain = id::<DeviceChain>(2_000 + index);
        doc = apply(
            &doc,
            &Command::new(CommandPayload::CreateTrack {
                id: track,
                chain_id: chain,
                kind: TrackKind::Audio,
                name: format!("Track {index}"),
                parent: None,
                index: None,
                color: None,
            }),
        )
        .unwrap()
        .doc;
        for (offset, type_id) in ["notwig.tool", "notwig.eq"].into_iter().enumerate() {
            let device_id = id::<Device>(10_000 + index * 10 + offset as u128);
            doc = apply(
                &doc,
                &Command::new(CommandPayload::InsertDevice {
                    id: device_id,
                    type_id: type_id.into(),
                    version: Some(SchemaVersion::V0),
                    kind: DeviceKind::AudioFx,
                    target_chain: chain,
                    index: None,
                    child_chain_ids: vec![],
                    name: None,
                }),
            )
            .unwrap()
            .doc;
        }
    }
    doc
}

fn bench_compile(c: &mut Criterion) {
    let caps = EngineCaps {
        sample_rate: SAMPLE_RATE,
        max_block_size: 256,
        channels: 2,
    };
    let registry: DeviceRegistry = stock_registry();
    let mut group = c.benchmark_group("graph/compile");
    for track_count in [1_u128, 8, 32] {
        let doc = graph_doc(track_count);
        group.bench_with_input(
            BenchmarkId::from_parameter(track_count),
            &track_count,
            |b, _| {
                b.iter(|| {
                    let plan = compile(black_box(&doc), black_box(&registry), caps).unwrap();
                    black_box(plan);
                });
            },
        );
    }
    group.finish();
}

fn bench_reference(c: &mut Criterion) {
    let mut group = c.benchmark_group("reference/pan_gains");
    group.throughput(Throughput::Elements(1));
    group.bench_function("single", |b| {
        b.iter(|| {
            let gains = notwig_core::Pan(black_box(0.25)).gains();
            black_box(gains);
        });
    });
    group.finish();
}

fn bench_construct(c: &mut Criterion) {
    let spec = tool_spec();
    let mut group = c.benchmark_group("stock/tool/construct");
    group.bench_function("prepare", |b| {
        b.iter(|| {
            let tool = tool_processor(black_box(&spec));
            black_box(tool);
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_tool,
    bench_eq,
    bench_eq_disabled,
    bench_chain,
    bench_eq_response,
    bench_compile,
    bench_reference,
    bench_construct,
);
criterion_main!(benches);
