use enum_map::EnumMap;

use crate::{context::Context, supported_language::SupportedLanguage, vex::Vex};

pub struct VexStore(EnumMap<SupportedLanguage, Vec<Vex>>);

impl VexMap {
    pub fn load(ctx: &Context) -> anyhow::Result<Self> {
        todo!()
    }

    pub fn vexes_for(&self, lang: SupportedLanguage) -> Option<&[Vex]> {
        todo!()
    }

    pub fn vexes(&self) -> impl Iterator<Item = &Vex> {
        self.0.values().flat_map(|v| v.iter())
    }
}
