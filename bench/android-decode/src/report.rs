use std::fmt::Write as _;

/// Latencies of one frame, in milliseconds from queueing its input.
pub struct FrameTiming {
    pub output_ms: f64,
    /// When the frame reached the Surface's consumer, if it was decoded to one.
    pub image_ms: Option<f64>,
    pub key: bool,
    pub bytes: usize,
}

fn percentile(sorted: &[f64], fraction: f64) -> f64 {
    let index = ((sorted.len() - 1) as f64 * fraction).round() as usize;
    sorted[index]
}

fn summary_line(label: &str, mut values: Vec<f64>) -> String {
    if values.is_empty() {
        return format!("{label:<8} no frames");
    }
    values.sort_by(f64::total_cmp);
    format!(
        "{label:<8} p10 {:>6.2}  p50 {:>6.2}  p90 {:>6.2}  p99 {:>6.2}  max {:>6.2} ms",
        percentile(&values, 0.1),
        percentile(&values, 0.5),
        percentile(&values, 0.9),
        percentile(&values, 0.99),
        values[values.len() - 1],
    )
}

pub fn summary(timings: &[FrameTiming], warmup: usize) -> String {
    let measured = timings.get(warmup..).unwrap_or_default();
    let mut text = format!("{} frames measured ({warmup} warmup)\n", measured.len());
    let output = measured.iter().map(|timing| timing.output_ms).collect();
    text.push_str(&summary_line("output", output));
    let images: Vec<f64> = measured
        .iter()
        .filter_map(|timing| timing.image_ms)
        .collect();
    if !images.is_empty() {
        let dropped = measured.len() - images.len();
        text.push('\n');
        text.push_str(&summary_line("surface", images));
        text.push_str(&format!("\n{dropped} frames never reached the Surface"));
    }
    text
}

pub fn csv(timings: &[FrameTiming]) -> String {
    let mut text = String::from("frame,key,bytes,output_ms,surface_ms\n");
    for (index, timing) in timings.iter().enumerate() {
        let image = timing
            .image_ms
            .map(|value| format!("{value:.2}"))
            .unwrap_or_default();
        let _ = writeln!(
            text,
            "{index},{},{},{:.2},{image}",
            timing.key as u8, timing.bytes, timing.output_ms
        );
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_skip_warmup() {
        let timings: Vec<FrameTiming> = (0..12)
            .map(|index| FrameTiming {
                output_ms: if index < 2 { 100.0 } else { index as f64 },
                image_ms: None,
                key: index == 0,
                bytes: 1,
            })
            .collect();
        let text = summary(&timings, 2);
        assert!(text.starts_with("10 frames measured"));
        assert!(text.contains("max  11.00"));
    }
}
