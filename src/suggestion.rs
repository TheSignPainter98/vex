pub fn suggest<'a>(input: &str, options: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    options
        .into_iter()
        .map(|option| (option, strsim::damerau_levenshtein(input, option)))
        .min_by_key(|(_, distance)| *distance)
        .and_then(|(nearest, distance)| if distance <= 2 { Some(nearest) } else { None })
}
