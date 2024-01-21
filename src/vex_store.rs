use enum_map::EnumMap;

use crate::{supported_language::SupportedLanguage, vex::Vex};

#[allow(unused)]
pub struct VexStore<'s> {
    vexes_for_lang: EnumMap<SupportedLanguage, Vec<Vex<'s>>>,
}

impl<'s> VexStore<'s> {
    #[allow(unused)]
    pub fn get(&self, _lang: SupportedLanguage) -> Option<&[Vex]> {
        todo!()
    }

    #[allow(unused)]
    pub fn all(&self) -> impl Iterator<Item = &Vex> {
        self.vexes_for_lang.values().flat_map(|vexes| vexes.iter())
    }
}
