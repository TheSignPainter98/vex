use enum_map::EnumMap;

use crate::{supported_language::SupportedLanguage, vex::Vex};

pub struct VexStore<'s> {
    vexes_for_lang: EnumMap<SupportedLanguage, Vec<Vex<'s>>>,
}

impl<'s> VexStore<'s> {
    pub fn get(&self, _lang: SupportedLanguage) -> Option<&[Vex]> {
        todo!()
    }

    pub fn all(&self) -> impl Iterator<Item = &Vex> {
        self.vexes_for_lang.values().flat_map(|vexes| vexes.iter())
    }
}
