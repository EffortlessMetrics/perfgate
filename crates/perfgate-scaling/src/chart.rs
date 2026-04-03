//! ASCII chart rendering for scaling analysis results.

use crate::models::ComplexityClass;
use crate::report::SizeMeasurement;

/// Render an ASCII chart showing measured data points and fitted curves.
///
/// The chart displays:
/// - '*' for actual measured data points
/// - '-' for the best-fit curve
/// - The complexity class label
///
/// # Arguments
///
/// * `measurements` - The measured data points
/// * `best_fit` - The best-fitting complexity class
/// * `coefficients` - The fitted coefficients for the best model
/// * `width` - Chart width in characters (default: 60)
/// * `height` - Chart height in lines (default: 20)
#[must_use = "pure computation; call site should use the returned chart string"]
pub fn render_ascii_chart(
    measurements: &[SizeMeasurement],
    best_fit: ComplexityClass,
    coefficients: &[f64],
    width: usize,
    height: usize,
) -> String {
    if measurements.is_empty() {
        return String::from("(no data)");
    }

    let width = width.max(20);
    let height = height.max(5);

    let min_n = measurements.iter().map(|m| m.input_size).min().unwrap_or(0) as f64;
    let max_n = measurements.iter().map(|m| m.input_size).max().unwrap_or(1) as f64;

    // Compute fitted values for the chart range
    let step = (max_n - min_n) / width as f64;
    let mut fitted_values = Vec::with_capacity(width);
    for i in 0..width {
        let n = min_n + step * i as f64;
        let y = best_fit.evaluate(n.max(1.0), coefficients);
        fitted_values.push(y);
    }

    // Combine actual and fitted values to determine y-axis range
    let all_y_values: Vec<f64> = measurements
        .iter()
        .map(|m| m.time_ms)
        .chain(fitted_values.iter().copied())
        .filter(|y| y.is_finite())
        .collect();

    if all_y_values.is_empty() {
        return String::from("(no finite data)");
    }

    let min_y = all_y_values
        .iter()
        .cloned()
        .fold(f64::INFINITY, f64::min)
        .max(0.0);
    let max_y = all_y_values
        .iter()
        .cloned()
        .fold(f64::NEG_INFINITY, f64::max);

    let y_range = if (max_y - min_y).abs() < f64::EPSILON {
        1.0
    } else {
        max_y - min_y
    };
    let n_range = if (max_n - min_n).abs() < f64::EPSILON {
        1.0
    } else {
        max_n - min_n
    };

    // Build chart grid
    let mut grid = vec![vec![' '; width]; height];

    // Plot fitted curve
    for (col, &fitted_y) in fitted_values.iter().enumerate() {
        if col < width && fitted_y.is_finite() {
            let row = ((max_y - fitted_y) / y_range * (height - 1) as f64)
                .round()
                .clamp(0.0, (height - 1) as f64) as usize;
            grid[row][col] = '-';
        }
    }

    // Plot actual data points (overwrite fitted curve)
    for m in measurements {
        let col = ((m.input_size as f64 - min_n) / n_range * (width - 1) as f64)
            .round()
            .clamp(0.0, (width - 1) as f64) as usize;
        let row = ((max_y - m.time_ms) / y_range * (height - 1) as f64)
            .round()
            .clamp(0.0, (height - 1) as f64) as usize;
        if col < width && row < height {
            grid[row][col] = '*';
        }
    }

    // Format y-axis labels
    let y_label_width = 10;
    let mut lines = Vec::with_capacity(height + 4);

    // Title
    lines.push(format!(
        "  Scaling Analysis: detected {} (R^2 shown in result)",
        best_fit
    ));
    lines.push(String::new());

    // Chart rows with y-axis labels
    for (row_idx, row) in grid.iter().enumerate() {
        let y_val = max_y - (row_idx as f64 / (height - 1) as f64) * y_range;
        let label = format_y_label(y_val, y_label_width);
        let row_str: String = row.iter().collect();
        if row_idx == 0 || row_idx == height - 1 || row_idx == height / 2 {
            lines.push(format!("{} |{}", label, row_str));
        } else {
            lines.push(format!("{} |{}", " ".repeat(y_label_width), row_str));
        }
    }

    // X-axis
    lines.push(format!(
        "{} +{}",
        " ".repeat(y_label_width),
        "-".repeat(width)
    ));

    // X-axis labels
    let min_label = format_size(min_n as u64);
    let max_label = format_size(max_n as u64);
    let mid_label = format_size(((min_n + max_n) / 2.0) as u64);
    let pad = width / 2 - mid_label.len() / 2;
    lines.push(format!(
        "{} {}{}{}{}",
        " ".repeat(y_label_width),
        min_label,
        " ".repeat(pad.saturating_sub(min_label.len())),
        mid_label,
        {
            let remaining = width.saturating_sub(min_label.len() + pad + mid_label.len());
            if remaining >= max_label.len() {
                format!("{}{}", " ".repeat(remaining - max_label.len()), max_label)
            } else {
                String::new()
            }
        }
    ));

    // Legend
    lines.push(String::new());
    lines.push(format!("  Legend: * = measured data, - = {} fit", best_fit));

    lines.join("\n")
}

fn format_y_label(value: f64, width: usize) -> String {
    let s = if value >= 1000.0 {
        format!("{:.1}s", value / 1000.0)
    } else if value >= 1.0 {
        format!("{:.1}ms", value)
    } else {
        format!("{:.3}ms", value)
    };
    format!("{:>width$}", s, width = width)
}

fn format_size(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{}M", n / 1_000_000)
    } else if n >= 1000 {
        format!("{}K", n / 1000)
    } else {
        format!("{}", n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_empty_data() {
        let chart = render_ascii_chart(&[], ComplexityClass::ON, &[1.0, 0.0], 40, 10);
        assert_eq!(chart, "(no data)");
    }

    #[test]
    fn render_basic_chart() {
        let measurements = vec![
            SizeMeasurement {
                input_size: 100,
                time_ms: 10.0,
            },
            SizeMeasurement {
                input_size: 200,
                time_ms: 20.0,
            },
            SizeMeasurement {
                input_size: 400,
                time_ms: 40.0,
            },
            SizeMeasurement {
                input_size: 800,
                time_ms: 80.0,
            },
        ];
        let chart = render_ascii_chart(&measurements, ComplexityClass::ON, &[0.1, 0.0], 40, 10);
        assert!(chart.contains('*'));
        assert!(chart.contains('-'));
        assert!(chart.contains("O(n)"));
    }

    #[test]
    fn render_chart_contains_legend() {
        let measurements = vec![
            SizeMeasurement {
                input_size: 10,
                time_ms: 5.0,
            },
            SizeMeasurement {
                input_size: 20,
                time_ms: 5.0,
            },
            SizeMeasurement {
                input_size: 30,
                time_ms: 5.0,
            },
        ];
        let chart = render_ascii_chart(&measurements, ComplexityClass::O1, &[5.0], 40, 10);
        assert!(chart.contains("Legend"));
        assert!(chart.contains("measured data"));
    }

    #[test]
    fn format_size_units() {
        assert_eq!(format_size(100), "100");
        assert_eq!(format_size(1000), "1K");
        assert_eq!(format_size(10000), "10K");
        assert_eq!(format_size(1_000_000), "1M");
        assert_eq!(format_size(5_000_000), "5M");
    }

    #[test]
    fn format_y_label_units() {
        let label = format_y_label(5.0, 10);
        assert!(label.contains("ms"));

        let label = format_y_label(1500.0, 10);
        assert!(label.contains("s"));

        let label = format_y_label(0.5, 10);
        assert!(label.contains("ms"));
    }

    #[test]
    fn render_chart_respects_minimum_dimensions() {
        let measurements = vec![
            SizeMeasurement {
                input_size: 10,
                time_ms: 1.0,
            },
            SizeMeasurement {
                input_size: 20,
                time_ms: 2.0,
            },
            SizeMeasurement {
                input_size: 30,
                time_ms: 3.0,
            },
        ];
        // Even with very small dimensions, should not panic
        let chart = render_ascii_chart(&measurements, ComplexityClass::ON, &[0.1, 0.0], 1, 1);
        assert!(!chart.is_empty());
    }
}
