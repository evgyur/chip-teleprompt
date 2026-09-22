//! Opt-in frame timing probe; records only timing/positions, never script text.
use std::{
    fs, io,
    path::{Path, PathBuf},
    time::Instant,
};

pub struct FrameProbe {
    directory: PathBuf,
    start: Instant,
    samples: Vec<(f64, f64, f64)>,
    completed: bool,
}

impl FrameProbe {
    pub fn new(directory: &Path) -> io::Result<Self> {
        fs::create_dir_all(directory)?;
        Ok(Self {
            directory: directory.into(),
            start: Instant::now(),
            samples: Vec::new(),
            completed: false,
        })
    }
    pub fn sample(&mut self, logical_y: f64, rendered_y: f64) -> io::Result<bool> {
        if self.completed {
            return Ok(false);
        }
        let milliseconds = self.start.elapsed().as_secs_f64() * 1000.0;
        self.samples.push((milliseconds, logical_y, rendered_y));
        if milliseconds < 3000.0 {
            return Ok(false);
        }
        self.completed = true;
        let mut csv = String::from("elapsed_ms,logical_y,rendered_y\n");
        for (time, y, pixel) in &self.samples {
            csv.push_str(&format!("{time:.4},{y:.6},{pixel:.6}\n"));
        }
        fs::write(self.directory.join("frames.csv"), csv)?;
        let mut intervals: Vec<f64> = self.samples.windows(2).map(|s| s[1].0 - s[0].0).collect();
        intervals.sort_by(f64::total_cmp);
        let percentile = |q: f64| {
            intervals
                .get(((intervals.len().saturating_sub(1)) as f64 * q).round() as usize)
                .copied()
                .unwrap_or(0.0)
        };
        let changed = self
            .samples
            .windows(2)
            .filter(|s| (s[1].2 - s[0].2).abs() > 0.0001)
            .count();
        let fps = 1000.0 * (self.samples.len().saturating_sub(1)) as f64
            / (milliseconds - self.samples[0].0);
        let report = format!("frames={}\nsubmission_fps={fps:.2}\ninterval_p50_ms={:.3}\ninterval_p95_ms={:.3}\ninterval_max_ms={:.3}\nposition_changes={changed}\n", self.samples.len(),percentile(0.5),percentile(0.95),percentile(1.0));
        fs::write(self.directory.join("timing.txt"), report)?;
        Ok(true)
    }
}
