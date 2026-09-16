#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenSize {
    pub width: u16,
    pub height: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Right,
    Left,
}

#[derive(Debug, Clone, Copy)]
pub struct Geometry {
    pub local: ScreenSize,
    pub peer: ScreenSize,
    pub side: Side,
}

fn last_index(dim: u16) -> f64 {
    f64::from(dim.saturating_sub(1))
}

fn map_y(y: f64, from_h: u16, to_h: u16) -> f64 {
    if from_h == 0 || to_h == 0 {
        return 0.0;
    }
    (y * last_index(to_h) / last_index(from_h)).round()
}

impl Geometry {
    pub fn entry_x(&self) -> f64 {
        match self.side {
            Side::Right => 0.0,
            Side::Left => last_index(self.peer.width),
        }
    }

    pub fn back_x(&self) -> f64 {
        match self.side {
            Side::Right => last_index(self.local.width),
            Side::Left => 0.0,
        }
    }

    pub fn map_out_y(&self, y: f64) -> f64 {
        map_y(y, self.local.height, self.peer.height)
    }

    pub fn map_back_y(&self, y: f64) -> f64 {
        map_y(y, self.peer.height, self.local.height)
    }

    pub fn is_exiting_local(&self, x: f64, dx: f64) -> bool {
        match self.side {
            Side::Right => x >= last_index(self.local.width) && dx > 0.0,
            Side::Left => x <= 0.0 && dx < 0.0,
        }
    }

    pub fn is_reentering(&self, virt_x: f64, dx: f64) -> bool {
        match self.side {
            Side::Right => virt_x <= 0.0 && dx < 0.0,
            Side::Left => virt_x >= last_index(self.peer.width) && dx > 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::geometry::{Geometry, ScreenSize, Side};

    fn g_1920_2560_right() -> Geometry {
        Geometry {
            local: ScreenSize { width: 1920, height: 1080 },
            peer: ScreenSize { width: 2560, height: 1440 },
            side: Side::Right,
        }
    }

    #[test]
    fn mapping_proportionnel() {
        let g = g_1920_2560_right();
        assert_eq!(g.map_out_y(0.0), 0.0);
        assert_eq!(g.map_out_y(540.0), 720.0);
        assert_eq!(g.map_out_y(1079.0), 1439.0);
        assert_eq!(g.map_back_y(720.0), 540.0);
    }

    #[test]
    fn entry_x_selon_side() {
        assert_eq!(g_1920_2560_right().entry_x(), 0.0);
        assert_eq!(g_1920_2560_right().back_x(), 1919.0);
        let left = Geometry {
            local: ScreenSize { width: 1920, height: 1080 },
            peer: ScreenSize { width: 2560, height: 1440 },
            side: Side::Left,
        };
        assert_eq!(left.entry_x(), 2559.0);
        assert_eq!(left.back_x(), 0.0);
    }

    #[test]
    fn detection_bord_direction() {
        let g = g_1920_2560_right();
        assert!(!g.is_exiting_local(1918.9, 0.001));
        assert!(g.is_exiting_local(1919.0, 0.001));
        assert!(!g.is_exiting_local(1919.0, -0.001));
        assert!(!g.is_exiting_local(100.0, 10.0));
        assert!(g.is_reentering(0.0, -1.0));
        assert!(!g.is_reentering(0.001, -1.0));
        assert!(!g.is_reentering(50.0, -1.0));
    }

    #[test]
    fn mapping_non_16_9() {
        let g = Geometry {
            local: ScreenSize { width: 1024, height: 768 },
            peer: ScreenSize { width: 3840, height: 2160 },
            side: Side::Right,
        };
        assert_eq!(g.map_out_y(384.0), 1081.0);
    }
}