use std::{str::FromStr, sync::LazyLock};

use chrono::Locale;

pub static SYS_LOCALE: LazyLock<Locale> = LazyLock::new(|| {
    match sys_locale::get_locale().map(|mut l| {
        while let Some(i) = l.find('-') {
            l.replace_range(i..i + '-'.len_utf8(), "_");
        }
        Locale::from_str(&l)
            .inspect_err(|_| {
                log::warn!(
                    "Failed to parse system locale '{}', using default instead",
                    l
                )
            })
            .unwrap_or_default()
    }) {
        Some(locale) => locale,
        None => {
            log::warn!("Failed to get system locale, using default");
            Locale::default()
        }
    }
});
