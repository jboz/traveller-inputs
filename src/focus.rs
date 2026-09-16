use crate::geometry::Geometry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Local,
    Remote,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StepOutcome {
    RemainLocal,
    EnterRemote { y: f64 },
    RemainRemote,
    ExitRemote { x: f64, y: f64 },
}

#[derive(Debug, Clone, Copy)]
pub struct FocusState {
    pub mode: Focus,
    pub virt_x: f64,
    pub virt_y: f64,
}

impl FocusState {
    pub fn new() -> Self {
        Self { mode: Focus::Local, virt_x: 0.0, virt_y: 0.0 }
    }

    pub fn is_remote(&self) -> bool {
        self.mode == Focus::Remote
    }

    pub fn force_local(&mut self) {
        self.mode = Focus::Local;
        self.virt_x = 0.0;
        self.virt_y = 0.0;
    }

    pub fn step(&mut self, geom: &Geometry, local_pos: (f64, f64), delta: (f64, f64)) -> StepOutcome {
        let (dx, dy) = delta;
        match self.mode {
            Focus::Local => {
                if geom.is_exiting_local(local_pos.0, dx) {
                    self.mode = Focus::Remote;
                    self.virt_x = geom.entry_x();
                    self.virt_y = geom.map_out_y(local_pos.1);
                    StepOutcome::EnterRemote { y: self.virt_y }
                } else {
                    StepOutcome::RemainLocal
                }
            }
            Focus::Remote => {
                self.virt_x += dx;
                self.virt_y = (self.virt_y + dy).clamp(0.0, (geom.peer.height.saturating_sub(1)) as f64);
                if geom.is_reentering(self.virt_x, dx) {
                    let back_y = self.virt_y;
                    let out_y = geom.map_back_y(back_y);
                    self.mode = Focus::Local;
                    self.virt_x = 0.0;
                    self.virt_y = 0.0;
                    StepOutcome::ExitRemote { x: geom.back_x(), y: out_y }
                } else {
                    StepOutcome::RemainRemote
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::focus::{FocusState, StepOutcome};
    use crate::geometry::{Geometry, ScreenSize, Side};

    fn geom() -> Geometry {
        Geometry {
            local: ScreenSize { width: 1920, height: 1080 },
            peer: ScreenSize { width: 1920, height: 1080 },
            side: Side::Right,
        }
    }

    #[test]
    fn cycle_local_distant_retour() {
        let mut fs = FocusState::new();
        let g = geom();
        assert_eq!(fs.step(&g, (500.0, 500.0), (10.0, 0.0)), StepOutcome::RemainLocal);
        assert_eq!(fs.step(&g, (1920.0, 540.0), (5.0, 0.0)), StepOutcome::EnterRemote { y: 540.0 });
        assert!(fs.is_remote());
        assert_eq!(fs.step(&g, (0.0, 0.0), (30.0, 0.0)), StepOutcome::RemainRemote);
        assert_eq!(fs.virt_x, 30.0);
        assert_eq!(fs.virt_y, 540.0);
        let out = fs.step(&g, (0.0, 0.0), (-35.0, 0.0));
        assert_eq!(out, StepOutcome::ExitRemote { x: 1919.0, y: 540.0 });
        assert!(!fs.is_remote());
    }

    #[test]
    fn jamais_deux_entrees_sans_sortie() {
        let mut fs = FocusState::new();
        let g = geom();
        for _ in 0..50 {
            let out = fs.step(&g, (1920.0, 500.0), (1.0, 0.0));
            assert!(matches!(out, StepOutcome::EnterRemote { .. }));
            assert!(fs.is_remote());
            let out = fs.step(&g, (0.0, 0.0), (-1.0, 0.0));
            assert!(matches!(out, StepOutcome::ExitRemote { .. }));
            assert!(!fs.is_remote());
        }
    }

    #[test]
    fn y_clampe_dans_l_ecran_peer() {
        let mut fs = FocusState::new();
        let g = geom();
        fs.step(&g, (1920.0, 100.0), (1.0, 0.0));
        assert!(matches!(fs.step(&g, (0.0, 0.0), (0.0, 2000.0)), StepOutcome::RemainRemote));
        assert_eq!(fs.virt_y, 1079.0);
    }
}