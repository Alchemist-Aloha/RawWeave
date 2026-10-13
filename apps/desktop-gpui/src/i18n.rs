//! Presentation translations; persisted graph identifiers never pass through this module.

use std::{collections::BTreeMap, sync::LazyLock};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Locale {
    #[default]
    English,
    SimplifiedChinese,
    TraditionalChinese,
    Spanish,
    French,
    German,
    Portuguese,
    Japanese,
    Korean,
    Russian,
}

impl Locale {
    pub const ALL: [Self; 10] = [
        Self::English,
        Self::SimplifiedChinese,
        Self::TraditionalChinese,
        Self::Spanish,
        Self::French,
        Self::German,
        Self::Portuguese,
        Self::Japanese,
        Self::Korean,
        Self::Russian,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::SimplifiedChinese => "zh-Hans",
            Self::TraditionalChinese => "zh-Hant",
            Self::Spanish => "es",
            Self::French => "fr",
            Self::German => "de",
            Self::Portuguese => "pt",
            Self::Japanese => "ja",
            Self::Korean => "ko",
            Self::Russian => "ru",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::SimplifiedChinese => "简体中文",
            Self::TraditionalChinese => "繁體中文",
            Self::Spanish => "Español",
            Self::French => "Français",
            Self::German => "Deutsch",
            Self::Portuguese => "Português",
            Self::Japanese => "日本語",
            Self::Korean => "한국어",
            Self::Russian => "Русский",
        }
    }

    pub fn parse(code: &str) -> Option<Self> {
        match code {
            "zh-CN" | "zh-SG" => Some(Self::SimplifiedChinese),
            "zh-TW" | "zh-HK" | "zh-MO" => Some(Self::TraditionalChinese),
            _ => Self::ALL.into_iter().find(|locale| locale.code() == code),
        }
    }

    fn translation_index(self) -> Option<usize> {
        Self::ALL
            .iter()
            .position(|locale| *locale == self)
            .and_then(|index| index.checked_sub(1))
    }
}

type Catalog = BTreeMap<String, Vec<String>>;

fn catalogs() -> &'static [Catalog; 3] {
    static CATALOGS: LazyLock<[Catalog; 3]> = LazyLock::new(|| {
        [
            include_str!("../assets/i18n/shell.json"),
            include_str!("../assets/i18n/panels.json"),
            include_str!("../assets/i18n/nodes.json"),
        ]
        .map(|json| serde_json::from_str(json).unwrap_or_default())
    });
    &CATALOGS
}

/// Unknown technical or third-party labels remain readable in their original form.
pub fn tr(locale: Locale, key: &str) -> &str {
    let Some(index) = locale.translation_index() else {
        return key;
    };
    catalogs()
        .iter()
        .find_map(|catalog| catalog.get(key).and_then(|values| values.get(index)))
        .filter(|value| !value.is_empty())
        .map_or(key, String::as_str)
}

/// Named substitutions keep sentence order under the translator's control.
pub fn message(locale: Locale, key: &str, values: &[(&str, &str)]) -> String {
    let template = tr(locale, key);
    let mut output = String::with_capacity(template.len());
    let mut remaining = template;
    while let Some(start) = remaining.find('{') {
        output.push_str(&remaining[..start]);
        let Some(end) = remaining[start..].find('}') else {
            output.push_str(&remaining[start..]);
            return output;
        };
        let end = start + end;
        let name = &remaining[start + 1..end];
        if let Some((_, value)) = values.iter().find(|(key, _)| *key == name) {
            output.push_str(value);
        } else {
            output.push_str(&remaining[start..=end]);
        }
        remaining = &remaining[end + 1..];
    }
    output.push_str(remaining);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_ui_translation_keys_are_present() {
        for source in [
            include_str!("main.rs"),
            include_str!("native_nodes.rs"),
            include_str!("native_scopes.rs"),
            include_str!("native_viewer.rs"),
            include_str!("geometry.rs"),
        ] {
            for (offset, _) in source
                .match_indices("tr(")
                .chain(source.match_indices("message("))
            {
                if offset > 0 && source.as_bytes()[offset - 1].is_ascii_alphanumeric()
                    || offset > 0 && source.as_bytes()[offset - 1] == b'_'
                {
                    continue;
                }
                let length = if source[offset..].starts_with("message(") {
                    8
                } else {
                    3
                };
                let call = &source[offset + length..];
                let Some((_, argument)) = call.split_once(',') else {
                    continue;
                };
                let argument = argument.trim_start();
                if !argument.starts_with('"') {
                    continue;
                }
                let mut escaped = false;
                let end = argument
                    .char_indices()
                    .skip(1)
                    .find_map(|(index, character)| {
                        if escaped {
                            escaped = false;
                            return None;
                        }
                        if character == '\\' {
                            escaped = true;
                            return None;
                        }
                        (character == '"').then_some(index)
                    })
                    .unwrap();
                let key: String = serde_json::from_str(&argument[..=end]).unwrap();
                assert!(
                    catalogs().iter().any(|catalog| catalog.contains_key(&key)),
                    "Missing UI key: {key}"
                );
            }
        }
    }

    #[test]
    fn registered_node_names_and_parameter_help_have_translations() {
        let mut missing = Vec::new();
        for node in crate::Session::default().editor.node_descriptors() {
            if !catalogs()
                .iter()
                .any(|catalog| catalog.contains_key(&node.name))
            {
                missing.push(node.name);
            }
        }
        let metadata: serde_json::Value =
            serde_json::from_str(include_str!("../assets/editor/parameter-ux.json")).unwrap();
        for entry in metadata["specific"].as_object().unwrap().values() {
            for field in ["name", "description"] {
                let key = entry[field].as_str().unwrap();
                if !catalogs().iter().any(|catalog| catalog.contains_key(key)) {
                    missing.push(key.to_owned());
                }
            }
        }
        assert!(
            missing.is_empty(),
            "Missing registered presentation strings: {missing:?}"
        );
    }

    #[test]
    fn ten_languages_have_complete_catalogs_and_distinct_chinese_scripts() {
        assert_eq!(Locale::ALL.len(), 10);
        assert_eq!(Locale::parse("zh-CN"), Some(Locale::SimplifiedChinese));
        assert_eq!(Locale::parse("zh-TW"), Some(Locale::TraditionalChinese));
        assert_eq!(Locale::parse("unknown"), None);
        assert_eq!(tr(Locale::English, "Open Image"), "Open Image");
        assert_eq!(tr(Locale::SimplifiedChinese, "Open Image"), "打开图像");
        assert_eq!(tr(Locale::TraditionalChinese, "Open Image"), "開啟影像");
        assert_eq!(
            tr(Locale::French, "unregistered technical detail"),
            "unregistered technical detail"
        );
        for catalog in catalogs() {
            assert!(!catalog.is_empty());
            for (key, values) in catalog {
                assert_eq!(values.len(), 9, "{key}");
                assert!(values.iter().all(|value| !value.trim().is_empty()), "{key}");
                let placeholders = |text: &str| {
                    text.split('{')
                        .skip(1)
                        .filter_map(|part| part.split_once('}').map(|(name, _)| name.to_owned()))
                        .collect::<std::collections::BTreeSet<_>>()
                };
                for value in values {
                    assert_eq!(placeholders(key), placeholders(value), "{key}: {value}");
                }
            }
        }
    }

    #[test]
    fn messages_substitute_without_reinterpreting_user_paths_as_placeholders() {
        assert_eq!(
            message(
                Locale::English,
                "Opening {path}…",
                &[("path", "{error}.jpg")]
            ),
            "Opening {error}.jpg…"
        );
        assert_eq!(
            message(
                Locale::SimplifiedChinese,
                "Opening {path}…",
                &[("path", "photo.jpg")]
            ),
            "正在打开 photo.jpg…"
        );
        assert_eq!(
            message(Locale::English, "{missing} {path}", &[("path", "test")]),
            "{missing} test"
        );
    }
}
