use mint_sage_livingworld::ArmyId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Signal {
    Deselect(ArmyId),
    Select(ArmyId),
    Battle(u32),
}

#[derive(Debug, Default)]
pub(super) struct Told {
    army: Option<ArmyId>,
    battle: Option<u32>,
}

impl Told {
    pub(super) fn changes(&mut self, army: Option<ArmyId>, battle: Option<u32>) -> Vec<Signal> {
        let mut out = Vec::new();
        if army != self.army {
            if let Some(old) = self.army.take() {
                out.push(Signal::Deselect(old));
            }
            if let Some(new) = army {
                out.push(Signal::Select(new));
                self.army = Some(new);
            }
        }
        if battle != self.battle {
            self.battle = battle;
            if let Some(id) = battle {
                out.push(Signal::Battle(id));
            }
        }
        out
    }

    pub(super) fn refused(&mut self) {
        self.army = None;
    }
}
