use mint_ecs::World;
use mint_math::rng::Pcg32;

use crate::command::{Command, CommandKey};
use crate::time::Frame;
use crate::Result;

pub struct Tick<'a> {
    world: &'a mut World,
    rng: &'a mut Pcg32,
    frame: Frame,
}

impl<'a> Tick<'a> {
    pub(crate) fn new(world: &'a mut World, rng: &'a mut Pcg32, frame: Frame) -> Self {
        Self { world, rng, frame }
    }

    #[must_use]
    pub const fn frame(&self) -> Frame {
        self.frame
    }

    #[must_use]
    pub fn state(&self) -> &World {
        self.world
    }

    #[must_use]
    pub fn state_mut(&mut self) -> &mut World {
        self.world
    }

    #[must_use]
    pub fn rng(&mut self) -> &mut Pcg32 {
        self.rng
    }

    pub fn state_and_rng_mut(&mut self) -> (&mut World, &mut Pcg32) {
        (self.world, self.rng)
    }
}

impl core::fmt::Debug for Tick<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Tick")
            .field("frame", &self.frame.get())
            .field("entities", &self.world.len())
            .finish_non_exhaustive()
    }
}

pub trait Rules {
    type Command: Command;

    fn setup(&mut self, tick: &mut Tick<'_>) -> Result<()> {
        let _ = tick;
        Ok(())
    }

    fn apply(&self, tick: &mut Tick<'_>, key: CommandKey, command: &Self::Command) -> Result<()>;

    fn step(&self, tick: &mut Tick<'_>) -> Result<()> {
        let _ = tick;
        Ok(())
    }
}
