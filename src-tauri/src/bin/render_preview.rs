use cyclemetry_core::activity::{build_dense_activity_report, parse_activity_json};
use cyclemetry_core::commands::AppPaths;
use cyclemetry_core::config::parse_config_json;
use cyclemetry_core::debug::TimingBucket;
use cyclemetry_core::render::{
    prepare_preview_assets, render_preview_with_prepared_assets, LabelCacheStatus,
    PreviewRenderReport,
};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

fn read_arg(flag: &str, args: &[String]) -> Result<String, String> {
    args.windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].clone())
        .ok_or_else(|| format!("Missing required argument: {flag}"))
}

fn repo_root() -> Result<PathBuf, String> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .map(PathBuf::from)
        .ok_or_else(|| "Failed to resolve repo root".to_string())
}

fn read_optional_arg(flag: &str, args: &[String]) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].clone())
}

fn parse_seconds(args: &[String]) -> Result<Vec<u32>, String> {
    if let Some(seconds) = read_optional_arg("--seconds", args) {
        let parsed = seconds
            .split(',')
            .filter(|value| !value.trim().is_empty())
            .map(|value| {
                value
                    .trim()
                    .parse::<u32>()
                    .map_err(|error| format!("Invalid second value '{value}': {error}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if parsed.is_empty() {
            return Err("--seconds must contain at least one value".to_string());
        }
        return Ok(parsed);
    }

    Ok(vec![read_arg("--second", args)?.parse::<u32>().map_err(
        |error| format!("Invalid --second value: {error}"),
    )?])
}

#[derive(Serialize)]
struct PreviewBatchSummary {
    frame_count: usize,
    seconds: Vec<u32>,
    total_ms_sum: f64,
    total_ms_avg: f64,
    total_ms_min: f64,
    total_ms_max: f64,
    surface_ms_avg: f64,
    label_layer_ms_avg: f64,
    value_draw_ms_avg: f64,
    png_write_ms_avg: f64,
    prepare_timings_avg: BTreeMap<String, TimingBucket>,
    frame_timings_avg: BTreeMap<String, TimingBucket>,
    preview_only_timings_avg: BTreeMap<String, TimingBucket>,
    reports: Vec<PreviewRenderReport>,
}

fn average_timing_buckets(
    reports: &[PreviewRenderReport],
    extractor: fn(&PreviewRenderReport) -> &BTreeMap<String, TimingBucket>,
) -> BTreeMap<String, TimingBucket> {
    let mut combined: BTreeMap<String, TimingBucket> = BTreeMap::new();
    for report in reports {
        for (name, bucket) in extractor(report) {
            let entry = combined.entry(name.clone()).or_default();
            entry.count += 1;
            entry.total_ms += bucket.total_ms;
            if bucket.max_ms > entry.max_ms {
                entry.max_ms = bucket.max_ms;
            }
        }
    }

    for bucket in combined.values_mut() {
        bucket.avg_ms = if bucket.count == 0 {
            0.0
        } else {
            bucket.total_ms / f64::from(bucket.count)
        };
    }

    combined
}

fn summarize_reports(reports: Vec<PreviewRenderReport>) -> PreviewBatchSummary {
    let frame_count = reports.len();
    let seconds = reports
        .iter()
        .map(|report| report.second)
        .collect::<Vec<_>>();
    let total_ms_sum = reports.iter().map(|report| report.total_ms).sum::<f64>();
    let average = |extractor: fn(&PreviewRenderReport) -> f64| {
        reports.iter().map(extractor).sum::<f64>() / frame_count as f64
    };

    PreviewBatchSummary {
        frame_count,
        seconds,
        total_ms_sum,
        total_ms_avg: total_ms_sum / frame_count as f64,
        total_ms_min: reports
            .iter()
            .map(|report| report.total_ms)
            .fold(f64::INFINITY, f64::min),
        total_ms_max: reports
            .iter()
            .map(|report| report.total_ms)
            .fold(f64::NEG_INFINITY, f64::max),
        surface_ms_avg: average(|report| report.surface_ms),
        label_layer_ms_avg: average(|report| report.label_layer_ms),
        value_draw_ms_avg: average(|report| report.value_draw_ms),
        png_write_ms_avg: average(|report| report.png_write_ms),
        prepare_timings_avg: average_timing_buckets(&reports, |report| &report.prepare_timings),
        frame_timings_avg: average_timing_buckets(&reports, |report| &report.frame_timings),
        preview_only_timings_avg: average_timing_buckets(&reports, |report| {
            &report.preview_only_timings
        }),
        reports,
    }
}

fn main() -> Result<(), String> {
    let args = std::env::args().collect::<Vec<_>>();
    let payload_path = PathBuf::from(read_arg("--payload", &args)?);
    let config_path = PathBuf::from(read_arg("--config", &args)?);
    let out_path = PathBuf::from(read_arg("--out", &args)?);
    let timing_out_path = read_optional_arg("--timing-out", &args).map(PathBuf::from);
    let seconds = parse_seconds(&args)?;

    let payload_json = fs::read_to_string(&payload_path)
        .map_err(|error| format!("Failed to read {}: {error}", payload_path.display()))?;
    let config_json = fs::read_to_string(&config_path)
        .map_err(|error| format!("Failed to read {}: {error}", config_path.display()))?;

    let activity = parse_activity_json(&payload_json)?;
    let config = parse_config_json(&config_json)?;
    let dense_activity = build_dense_activity_report(&activity, &config)?;

    let paths = AppPaths::from_repo_root(repo_root()?);
    let (prepared_preview_assets, label_cache_status, prepare_timings, prepare_total_ms) =
        prepare_preview_assets(&paths, &config, &activity, &dense_activity)?;
    let mut reports = Vec::with_capacity(seconds.len());
    for (index, second) in seconds.into_iter().enumerate() {
        let target_out_path = if reports.is_empty() {
            out_path.clone()
        } else {
            let stem = out_path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("preview");
            let extension = out_path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("png");
            out_path.with_file_name(format!("{stem}_{second}.{extension}"))
        };

        if let Some(parent) = target_out_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Failed to create {}: {error}", parent.display()))?;
        }

        let prepare_timings_for_frame = if index == 0 {
            prepare_timings.clone()
        } else {
            BTreeMap::new()
        };
        let per_frame_label_cache_status = if index == 0 {
            label_cache_status
        } else {
            match label_cache_status {
                LabelCacheStatus::None => LabelCacheStatus::None,
                LabelCacheStatus::Hit | LabelCacheStatus::Miss => LabelCacheStatus::Hit,
            }
        };
        let extra_total_ms = if index == 0 { prepare_total_ms } else { 0.0 };

        let (_, report) = render_preview_with_prepared_assets(
            &paths,
            &config,
            &dense_activity,
            &prepared_preview_assets,
            second,
            prepare_timings_for_frame,
            per_frame_label_cache_status,
            extra_total_ms,
            &target_out_path,
        )?;
        reports.push(report);
    }
    let summary = summarize_reports(reports);

    if let Some(timing_out_path) = timing_out_path {
        if let Some(parent) = timing_out_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Failed to create {}: {error}", parent.display()))?;
        }
        let timing_json = serde_json::to_string_pretty(&summary)
            .map_err(|error| format!("Failed to serialize timing report: {error}"))?;
        fs::write(&timing_out_path, timing_json)
            .map_err(|error| format!("Failed to write {}: {error}", timing_out_path.display()))?;
    }

    let stdout = serde_json::to_string_pretty(&summary)
        .map_err(|error| format!("Failed to serialize timing report: {error}"))?;
    println!("{stdout}");
    Ok(())
}
