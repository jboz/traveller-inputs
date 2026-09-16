#[derive(Debug, Default)]
pub struct Coalescer {
    pending_x: f64,
    pending_y: f64,
    has: bool,
}

impl Coalescer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, dx: f64, dy: f64) {
        self.pending_x += dx;
        self.pending_y += dy;
        self.has = true;
    }

    pub fn take(&mut self) -> Option<(f64, f64)> {
        if !self.has {
            return None;
        }
        self.has = false;
        Some((
            std::mem::take(&mut self.pending_x),
            std::mem::take(&mut self.pending_y),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poussee_accumule_puis_take() {
        let mut c = Coalescer::new();
        assert_eq!(c.take(), None);
        c.push(1.0, 2.0);
        c.push(0.5, -1.0);
        c.push(-2.0, 3.0);
        assert_eq!(c.take(), Some((-0.5, 4.0)));
        assert_eq!(c.take(), None);
    }
}