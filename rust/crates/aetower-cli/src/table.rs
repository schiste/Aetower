//! A deliberately tiny fixed-width table renderer.
//!
//! The repo keeps its dependency surface lean (no `comfy-table` etc.), and the
//! CLI's needs are modest: pad columns to their widest cell, honor per-column
//! alignment, and separate the header. Unicode width is approximated by
//! `char` count, which is correct for the ASCII/emoji-free report fields we
//! render; anything richer would be over-engineering for operator output.

/// Column alignment.
#[derive(Clone, Copy)]
pub enum Align {
    Left,
    Right,
}

/// A column: a header plus how its cells align.
pub struct Column {
    pub header: &'static str,
    pub align: Align,
}

impl Column {
    pub fn left(header: &'static str) -> Self {
        Self {
            header,
            align: Align::Left,
        }
    }

    pub fn right(header: &'static str) -> Self {
        Self {
            header,
            align: Align::Right,
        }
    }
}

/// Render `rows` under `columns` as a padded table with a header rule.
/// Returns an empty-state line when there are no rows.
pub fn render(columns: &[Column], rows: &[Vec<String>], empty: &str) -> String {
    if rows.is_empty() {
        return empty.to_string();
    }

    let mut widths: Vec<usize> = columns.iter().map(|c| c.header.chars().count()).collect();
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i < widths.len() {
                widths[i] = widths[i].max(cell.chars().count());
            }
        }
    }

    let mut out = String::new();
    push_row(
        &mut out,
        &widths,
        columns,
        &columns
            .iter()
            .map(|c| c.header.to_string())
            .collect::<Vec<_>>(),
    );
    // Header rule.
    let rule: Vec<String> = widths.iter().map(|w| "─".repeat(*w)).collect();
    push_row(&mut out, &widths, columns, &rule);
    for row in rows {
        push_row(&mut out, &widths, columns, row);
    }
    // Trim the trailing newline so callers can `println!` cleanly.
    if out.ends_with('\n') {
        out.pop();
    }
    out
}

fn push_row(out: &mut String, widths: &[usize], columns: &[Column], cells: &[String]) {
    let mut line = String::new();
    for (i, width) in widths.iter().enumerate() {
        let cell = cells.get(i).map(String::as_str).unwrap_or("");
        let align = columns.get(i).map(|c| c.align).unwrap_or(Align::Left);
        let pad = width.saturating_sub(cell.chars().count());
        match align {
            Align::Left => {
                line.push_str(cell);
                if i + 1 < widths.len() {
                    line.push_str(&" ".repeat(pad));
                }
            }
            Align::Right => {
                line.push_str(&" ".repeat(pad));
                line.push_str(cell);
            }
        }
        if i + 1 < widths.len() {
            line.push_str("  ");
        }
    }
    // Drop trailing spaces from the final (left-aligned) column.
    while line.ends_with(' ') {
        line.pop();
    }
    out.push_str(&line);
    out.push('\n');
}

/// Format a byte count as a compact human string (e.g. `11.2 GB`).
pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    if bytes == 0 {
        return "0 B".to_string();
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_padded_columns_with_alignment() {
        let cols = [Column::left("NAME"), Column::right("N")];
        let rows = vec![
            vec!["chrome".to_string(), "48".to_string()],
            vec!["ollama".to_string(), "3".to_string()],
        ];
        let out = render(&cols, &rows, "none");
        let lines: Vec<&str> = out.lines().collect();
        // NAME padded to width 6, 2-space gutter, then "N" right-aligned in width 2.
        assert_eq!(lines[0], "NAME     N");
        assert_eq!(lines[2], "chrome  48");
        // Right-aligned single digit lines up under the two-digit value.
        assert_eq!(lines[3], "ollama   3");
    }

    #[test]
    fn empty_rows_use_empty_state() {
        let cols = [Column::left("NAME")];
        assert_eq!(render(&cols, &[], "nothing here"), "nothing here");
    }

    #[test]
    fn human_bytes_scales() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1536), "1.5 KB");
        assert_eq!(human_bytes(12_026_531_840), "11.2 GB");
    }
}
