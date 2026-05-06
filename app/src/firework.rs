use std::f64::consts::PI;
use std::time::{Duration, Instant};

const DURATION: Duration = Duration::from_millis(900);
const PARTICLE_COUNT: usize = 28;

#[derive(Debug, Clone)]
pub struct Particle {
    pub x: f64,
    pub y: f64,
    pub dx: f64,
    pub dy: f64,
    pub color_idx: usize,
    pub ch: char,
}

pub struct Firework {
    pub particles: Vec<Particle>,
    started: Instant,
}

// ANSI 256-color indices: red, orange, yellow, green, cyan, magenta
pub const COLORS: [u8; 6] = [196, 208, 226, 46, 51, 201];
const CHARS: [char; 6] = ['*', '+', '·', '✦', '✸', '★'];

impl Firework {
    pub fn new() -> Self {
        let mut particles = Vec::with_capacity(PARTICLE_COUNT);
        for i in 0..PARTICLE_COUNT {
            let angle = (i as f64 / PARTICLE_COUNT as f64) * 2.0 * PI;
            let speed = 0.3 + (i % 4) as f64 * 0.12;
            particles.push(Particle {
                x: 0.0,
                y: 0.0,
                dx: angle.cos() * speed,
                dy: angle.sin() * speed * 0.5,
                color_idx: i % COLORS.len(),
                ch: CHARS[i % CHARS.len()],
            });
        }
        Self {
            particles,
            started: Instant::now(),
        }
    }

    pub fn tick(&mut self) {
        for p in &mut self.particles {
            p.x += p.dx;
            p.y += p.dy;
        }
    }

    pub fn done(&self) -> bool {
        self.started.elapsed() >= DURATION
    }

    #[allow(dead_code)]
    pub fn progress(&self) -> f64 {
        let elapsed = self.started.elapsed().as_secs_f64();
        (elapsed / DURATION.as_secs_f64()).min(1.0)
    }
}
