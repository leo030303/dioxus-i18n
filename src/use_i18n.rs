use super::error::Error;

use dioxus_lib::prelude::*;
use fluent::{FluentArgs, FluentBundle, FluentResource};
use unic_langid::LanguageIdentifier;

#[cfg(not(target_arch = "wasm32"))]
use walkdir::WalkDir;

use std::collections::HashMap;

#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};

/// `Locale` is a "place-holder" around what will eventually be a `fluent::FluentBundle`
#[cfg_attr(test, derive(Debug, PartialEq))]
pub struct Locale {
    id: LanguageIdentifier,
    resource: LocaleResource,
}

impl Locale {
    pub fn new_static(id: LanguageIdentifier, str: &'static str) -> Self {
        Self {
            id,
            resource: LocaleResource::Static(str),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn new_dynamic(id: LanguageIdentifier, path: impl Into<PathBuf>) -> Self {
        Self {
            id,
            resource: LocaleResource::Path(path.into()),
        }
    }
}

impl<T> From<(LanguageIdentifier, T)> for Locale
where
    T: Into<LocaleResource>,
{
    fn from((id, resource): (LanguageIdentifier, T)) -> Self {
        let resource = resource.into();
        Self { id, resource }
    }
}

/// A `LocaleResource` can be static text, or a filesystem file (not supported in WASM).
#[derive(Debug, PartialEq)]
pub enum LocaleResource {
    Static(&'static str),
    #[cfg(not(target_arch = "wasm32"))]
    Path(PathBuf),
}

impl LocaleResource {
    pub fn try_to_resource_string(&self) -> Result<String, Error> {
        match self {
            Self::Static(str) => Ok(str.to_string()),
            #[cfg(not(target_arch = "wasm32"))]
            Self::Path(path) => std::fs::read_to_string(path)
                .map_err(|e| Error::LocaleResourcePathReadFailed(e.to_string())),
        }
    }

    pub fn to_resource_string(&self) -> String {
        let result = self.try_to_resource_string();
        match result {
            Ok(string) => string,
            Err(err) => panic!("failed to create resource string {:?}: {}", self, err),
        }
    }
}

impl From<&'static str> for LocaleResource {
    fn from(value: &'static str) -> Self {
        Self::Static(value)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl From<PathBuf> for LocaleResource {
    fn from(value: PathBuf) -> Self {
        Self::Path(value)
    }
}

/// The configuration for `I18n`.
#[cfg_attr(test, derive(Debug, PartialEq))]
pub struct I18nConfig {
    /// The initial language, can be later changed with [`I18n::set_language`]
    id: LanguageIdentifier,

    /// The final fallback language if no other locales are found for `id`.
    /// A `Locale` must exist in `locales' if `fallback` is defined.
    fallback: Option<LanguageIdentifier>,

    /// The locale_resources added to the configuration.
    locale_resources: Vec<LocaleResource>,

    /// The locales added to the configuration.
    locales: HashMap<LanguageIdentifier, usize>,
}

impl I18nConfig {
    /// Create an i18n config with the selected [LanguageIdentifier].
    pub fn new(id: LanguageIdentifier) -> Self {
        Self {
            id,
            fallback: None,
            locale_resources: Vec::new(),
            locales: HashMap::new(),
        }
    }

    /// Set a fallback [LanguageIdentifier].
    pub fn with_fallback(mut self, fallback: LanguageIdentifier) -> Self {
        self.fallback = Some(fallback);
        self
    }

    /// Add [Locale].
    /// It is possible to share locales resources. If this locale's resource
    /// matches a previously added one, then this locale will use the existing one.
    /// This is primarily for the static locale_resources to avoid string duplication.
    pub fn with_locale<T>(mut self, locale: T) -> Self
    where
        T: Into<Locale>,
    {
        let locale = locale.into();
        let locale_resources_len = self.locale_resources.len();

        let index = self
            .locale_resources
            .iter()
            .position(|r| *r == locale.resource)
            .unwrap_or(locale_resources_len);

        if index == locale_resources_len {
            self.locale_resources.push(locale.resource)
        };

        self.locales.insert(locale.id, index);
        self
    }

    /// Add multiple locales from given folder, based on their filename.
    ///
    /// If the path represents a folder, then the folder will be deep traversed for
    /// all '*.ftl' files. If the filename represents a [LanguageIdentifier] then it
    ///  will be added to the config.
    ///
    /// If the path represents a file, then the filename must represent a
    /// unic_langid::LanguageIdentifier for it to be added to the config.
    ///
    /// The method is not available for `wasm32` builds.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn try_with_auto_locales(self, path: PathBuf) -> Result<Self, Error> {
        if path.is_dir() {
            let files = find_ftl_files(&path)?;
            files
                .into_iter()
                .try_fold(self, |acc, file| acc.with_auto_pathbuf(file))
        } else if is_ftl_file(&path) {
            self.with_auto_pathbuf(path)
        } else {
            Err(Error::InvalidPath(path.to_string_lossy().to_string()))
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn with_auto_pathbuf(self, file: PathBuf) -> Result<Self, Error> {
        assert!(is_ftl_file(&file));

        let stem = file.file_stem().ok_or_else(|| {
            Error::InvalidLanguageId(format!("No file stem: '{}'", file.display()))
        })?;

        let id_str = stem.to_str().ok_or_else(|| {
            Error::InvalidLanguageId(format!("Cannot convert: {}", stem.to_string_lossy()))
        })?;

        let id = LanguageIdentifier::from_bytes(id_str.as_bytes())
            .map_err(|e| Error::InvalidLanguageId(e.to_string()))?;

        Ok(self.with_locale((id, file)))
    }

    /// Add multiple locales from given folder, based on their filename.
    ///
    /// Will panic! on error.
    ///
    /// The method is not available for `wasm32` builds.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn with_auto_locales(self, path: PathBuf) -> Self {
        let path_name = path.display().to_string();
        let result = self.try_with_auto_locales(path);
        match result {
            Ok(result) => result,
            Err(err) => panic!(
                "with_auto_locales must have valid pathbuf {}: {}",
                path_name, err
            ),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn find_ftl_files(folder: &PathBuf) -> Result<Vec<PathBuf>, Error> {
    let ftl_files: Vec<PathBuf> = WalkDir::new(folder)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| is_ftl_file(entry.path()))
        .map(|entry| entry.path().to_path_buf())
        .collect();

    Ok(ftl_files)
}

#[cfg(not(target_arch = "wasm32"))]
fn is_ftl_file(entry: &Path) -> bool {
    entry.is_file() && entry.extension().map(|ext| ext == "ftl").unwrap_or(false)
}

/// Initialize an i18n provider.
pub fn try_use_init_i18n(init: impl FnOnce() -> I18nConfig) -> Result<I18n, Error> {
    use_context_provider(move || {
        // Coverage false -ve: See https://github.com/xd009642/tarpaulin/issues/1675
        let I18nConfig {
            id,
            fallback,
            locale_resources,
            locales,
        } = init();

        I18n::try_new(id, fallback, locale_resources, locales)
    })
}

/// Initialize an i18n provider.
pub fn use_init_i18n(init: impl FnOnce() -> I18nConfig) -> I18n {
    use_context_provider(move || {
        // Coverage false -ve: See https://github.com/xd009642/tarpaulin/issues/1675
        let I18nConfig {
            id,
            fallback,
            locale_resources,
            locales,
        } = init();

        match I18n::try_new(id, fallback, locale_resources, locales) {
            Ok(i18n) => i18n,
            Err(e) => panic!("Failed to create I18n context: {}", e),
        }
    })
}

#[derive(Clone, Copy)]
pub struct I18n {
    selected_language: Signal<LanguageIdentifier>,
    fallback_language: Signal<Option<LanguageIdentifier>>,
    locale_resources: Signal<Vec<LocaleResource>>,
    locales: Signal<HashMap<LanguageIdentifier, usize>>,
    active_bundle: Signal<FluentBundle<FluentResource>>,
}

impl I18n {
    pub fn try_new(
        selected_language: LanguageIdentifier,
        fallback_language: Option<LanguageIdentifier>,
        locale_resources: Vec<LocaleResource>,
        locales: HashMap<LanguageIdentifier, usize>,
    ) -> Result<Self, Error> {
        let bundle = try_create_bundle(
            &selected_language,
            &fallback_language,
            &locale_resources,
            &locales,
        )?;
        Ok(Self {
            selected_language: Signal::new(selected_language),
            fallback_language: Signal::new(fallback_language),
            locale_resources: Signal::new(locale_resources),
            locales: Signal::new(locales),
            active_bundle: Signal::new(bundle),
        })
    }

    pub fn new(
        selected_language: LanguageIdentifier,
        fallback_language: Option<LanguageIdentifier>,
        locale_resources: Vec<LocaleResource>,
        locales: HashMap<LanguageIdentifier, usize>,
    ) -> Self {
        let result = Self::try_new(
            selected_language,
            fallback_language,
            locale_resources,
            locales,
        );
        match result {
            Ok(i18n) => i18n,
            Err(err) => panic!("I18n cannot be created: {}", err),
        }
    }

    pub fn try_translate_with_args(
        &self,
        msg: &str,
        args: Option<&FluentArgs>,
    ) -> Result<String, Error> {
        let (message_id, attribute_name) = Self::decompose_identifier(msg)?;

        let bundle = self.active_bundle.read();

        let message = bundle
            .get_message(message_id)
            .ok_or_else(|| Error::MessageIdNotFound(message_id.into()))?;

        let pattern = if let Some(attribute_name) = attribute_name {
            let attribute = message
                .get_attribute(attribute_name)
                .ok_or_else(|| Error::AttributeIdNotFound(msg.to_string()))?;
            attribute.value()
        } else {
            message
                .value()
                .ok_or_else(|| Error::MessagePatternNotFound(message_id.into()))?
        };

        let mut errors = vec![];
        let translation = bundle
            .format_pattern(pattern, args, &mut errors)
            .to_string();

        (errors.is_empty())
            .then_some(translation)
            .ok_or_else(|| Error::FluentErrorsDetected(format!("{:#?}", errors)))
    }

    pub fn decompose_identifier(msg: &str) -> Result<(&str, Option<&str>), Error> {
        let parts: Vec<&str> = msg.split('.').collect();
        match parts.as_slice() {
            [message_id] => Ok((message_id, None)),
            [message_id, attribute_name] => Ok((message_id, Some(attribute_name))),
            _ => Err(Error::InvalidMessageId(msg.to_string())),
        }
    }

    pub fn translate_with_args(&self, msg: &str, args: Option<&FluentArgs>) -> String {
        let result = self.try_translate_with_args(msg, args);
        match result {
            Ok(translation) => translation,
            Err(err) => panic!("Failed to translate {}: {}", msg, err),
        }
    }

    #[inline]
    pub fn try_translate(&self, msg: &str) -> Result<String, Error> {
        self.try_translate_with_args(msg, None)
    }

    pub fn translate(&self, msg: &str) -> String {
        let result = self.try_translate(msg);
        match result {
            Ok(translation) => translation,
            Err(err) => panic!("Failed to translate {}: {}", msg, err),
        }
    }

    /// Get the selected language.
    #[inline]
    pub fn language(&self) -> LanguageIdentifier {
        self.selected_language.read().clone()
    }

    /// Get the fallback language.
    pub fn fallback_language(&self) -> Option<LanguageIdentifier> {
        self.fallback_language.read().clone()
    }

    /// Update the selected language.
    pub fn try_set_language(&mut self, id: LanguageIdentifier) -> Result<(), Error> {
        *self.selected_language.write() = id;
        self.try_update_active_bundle()
    }

    /// Update the selected language.
    pub fn set_language(&mut self, id: LanguageIdentifier) {
        let id_name = id.to_string();
        let result = self.try_set_language(id);
        match result {
            Ok(()) => (),
            Err(err) => panic!("cannot set language {}: {}", id_name, err),
        }
    }

    /// Update the fallback language.
    pub fn try_set_fallback_language(&mut self, id: LanguageIdentifier) -> Result<(), Error> {
        self.locales
            .read()
            .get(&id)
            .ok_or_else(|| Error::FallbackMustHaveLocale(id.to_string()))?;

        *self.fallback_language.write() = Some(id);
        self.try_update_active_bundle()
    }

    /// Update the fallback language.
    pub fn set_fallback_language(&mut self, id: LanguageIdentifier) {
        let id_name = id.to_string();
        let result = self.try_set_fallback_language(id);
        match result {
            Ok(()) => (),
            Err(err) => panic!("cannot set fallback language {}: {}", id_name, err),
        }
    }

    fn try_update_active_bundle(&mut self) -> Result<(), Error> {
        let bundle = try_create_bundle(
            &self.selected_language.peek(),
            &self.fallback_language.peek(),
            &self.locale_resources.peek(),
            &self.locales.peek(),
        )?;

        self.active_bundle.set(bundle);
        Ok(())
    }
}

fn try_create_bundle(
    selected_language: &LanguageIdentifier,
    fallback_language: &Option<LanguageIdentifier>,
    locale_resources: &[LocaleResource],
    locales: &HashMap<LanguageIdentifier, usize>,
) -> Result<FluentBundle<FluentResource>, Error> {
    let add_resource = move |bundle: &mut FluentBundle<FluentResource>,
                             langid: &LanguageIdentifier,
                             locale_resources: &[LocaleResource]| {
        if let Some(&i) = locales.get(langid) {
            let resource = &locale_resources[i];
            let resource =
                FluentResource::try_new(resource.try_to_resource_string()?).map_err(|e| {
                    Error::FluentErrorsDetected(format!("resource langid: {}\n{:#?}", langid, e))
                })?;
            bundle.add_resource_overriding(resource);
        };
        Ok(())
    };

    let mut bundle = FluentBundle::new(vec![selected_language.clone()]);
    if let Some(fallback_language) = fallback_language {
        add_resource(&mut bundle, fallback_language, locale_resources)?;
    }

    let (language, script, region, variants) = selected_language.clone().into_parts();
    let variants_lang = LanguageIdentifier::from_parts(language, script, region, &variants);
    let region_lang = LanguageIdentifier::from_parts(language, script, region, &[]);
    let script_lang = LanguageIdentifier::from_parts(language, script, None, &[]);
    let language_lang = LanguageIdentifier::from_parts(language, None, None, &[]);

    add_resource(&mut bundle, &language_lang, locale_resources)?;
    add_resource(&mut bundle, &script_lang, locale_resources)?;
    add_resource(&mut bundle, &region_lang, locale_resources)?;
    add_resource(&mut bundle, &variants_lang, locale_resources)?;

    /* Add this code when the fluent crate includes FluentBundle::add_builtins.
     * This will allow the use of built-in functions like `NUMBER` and `DATETIME`.
     * See [Fluent issue](https://github.com/projectfluent/fluent-rs/issues/181) for more information.
    bundle
        .add_builtins()
        .map_err(|e| Error::FluentErrorsDetected(e.to_string()))?;
    */

    Ok(bundle)
}

pub fn i18n() -> I18n {
    consume_context()
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;
    use unic_langid::langid;

    #[test]
    fn can_add_locale_to_config_explicit_locale() {
        const LANG_A: LanguageIdentifier = langid!("la-LA");
        const LANG_B: LanguageIdentifier = langid!("la-LB");
        const LANG_C: LanguageIdentifier = langid!("la-LC");

        let config = I18nConfig::new(LANG_A)
            .with_locale(Locale::new_static(LANG_B, "lang = lang_b"))
            .with_locale(Locale::new_dynamic(LANG_C, PathBuf::new()));

        assert_eq!(
            config,
            I18nConfig {
                id: LANG_A,
                fallback: None,
                locale_resources: vec![
                    LocaleResource::Static("lang = lang_b"),
                    LocaleResource::Path(PathBuf::new()),
                ],
                locales: HashMap::from([(LANG_B, 0), (LANG_C, 1)]),
            }
        );
    }

    #[test]
    fn can_add_locale_to_config_implicit_locale() {
        const LANG_A: LanguageIdentifier = langid!("la-LA");
        const LANG_B: LanguageIdentifier = langid!("la-LB");
        const LANG_C: LanguageIdentifier = langid!("la-LC");

        let config = I18nConfig::new(LANG_A)
            .with_locale((LANG_B, "lang = lang_b"))
            .with_locale((LANG_C, PathBuf::new()));

        assert_eq!(
            config,
            I18nConfig {
                id: LANG_A,
                fallback: None,
                locale_resources: vec![
                    LocaleResource::Static("lang = lang_b"),
                    LocaleResource::Path(PathBuf::new())
                ],
                locales: HashMap::from([(LANG_B, 0), (LANG_C, 1)]),
            }
        );
    }

    #[test]
    fn can_add_locale_string_to_config() {
        const LANG_A: LanguageIdentifier = langid!("la-LA");
        const LANG_B: LanguageIdentifier = langid!("la-LB");

        let config = I18nConfig::new(LANG_A).with_locale((LANG_B, "lang = lang_b"));

        assert_eq!(
            config,
            I18nConfig {
                id: LANG_A,
                fallback: None,
                locale_resources: vec![LocaleResource::Static("lang = lang_b")],
                locales: HashMap::from([(LANG_B, 0)]),
            }
        );
    }

    #[test]
    fn can_add_shared_locale_string_to_config() {
        const LANG_A: LanguageIdentifier = langid!("la-LA");
        const LANG_B: LanguageIdentifier = langid!("la-LB");
        const LANG_C: LanguageIdentifier = langid!("la-LC");

        let shared_string = "lang = a language";
        let config = I18nConfig::new(LANG_A)
            .with_locale((LANG_B, shared_string))
            .with_locale((LANG_C, shared_string));

        assert_eq!(
            config,
            I18nConfig {
                id: LANG_A,
                fallback: None,
                locale_resources: vec![LocaleResource::Static(shared_string)],
                locales: HashMap::from([(LANG_B, 0), (LANG_C, 0)]),
            }
        );
    }

    #[test]
    fn can_add_locale_pathbuf_to_config() {
        const LANG_A: LanguageIdentifier = langid!("la-LA");
        const LANG_C: LanguageIdentifier = langid!("la-LC");

        let config = I18nConfig::new(LANG_A)
            .with_locale((LANG_C, PathBuf::from("./test/data/fallback/la.ftl")));

        assert_eq!(
            config,
            I18nConfig {
                id: LANG_A,
                fallback: None,
                locale_resources: vec![LocaleResource::Path(PathBuf::from(
                    "./test/data/fallback/la.ftl"
                ))],
                locales: HashMap::from([(LANG_C, 0)]),
            }
        );
    }

    #[test]
    fn can_add_shared_locale_pathbuf_to_config() {
        const LANG_A: LanguageIdentifier = langid!("la-LA");
        const LANG_B: LanguageIdentifier = langid!("la-LB");
        const LANG_C: LanguageIdentifier = langid!("la-LC");

        let shared_pathbuf = PathBuf::from("./test/data/fallback/la.ftl");

        let config = I18nConfig::new(LANG_A)
            .with_locale((LANG_B, shared_pathbuf.clone()))
            .with_locale((LANG_C, shared_pathbuf.clone()));

        assert_eq!(
            config,
            I18nConfig {
                id: LANG_A,
                fallback: None,
                locale_resources: vec![LocaleResource::Path(shared_pathbuf)],
                locales: HashMap::from([(LANG_B, 0), (LANG_C, 0)]),
            }
        );
    }

    #[test]
    fn can_auto_add_locales_folder_to_config() {
        const LANG_A: LanguageIdentifier = langid!("la-LA");

        let root_path_str = &format!("{}/tests/data/fallback/", env!("CARGO_MANIFEST_DIR"));
        let pathbuf = PathBuf::from(root_path_str);

        let config = I18nConfig::new(LANG_A)
            .try_with_auto_locales(pathbuf)
            .ok()
            .unwrap();

        let expected_locales = [
            "fb-FB",
            "la",
            "la-Scpt",
            "la-Scpt-LA",
            "la-Scpt-LA-variants",
        ];

        assert_eq!(config.locales.len(), expected_locales.len());
        assert_eq!(config.locale_resources.len(), expected_locales.len());

        expected_locales.into_iter().for_each(|l| {
            let expected_filename = format!("{root_path_str}/{l}.ftl");
            let id = LanguageIdentifier::from_bytes(l.as_bytes()).unwrap();
            assert!(config.locales.get(&id).is_some());
            assert!(config
                .locale_resources
                .contains(&LocaleResource::Path(PathBuf::from(expected_filename))));
        });
    }

    #[test]
    fn can_auto_add_locales_file_to_config() {
        const LANG_A: LanguageIdentifier = langid!("la-LA");

        let path_str = &format!(
            "{}/tests/data/fallback/fb-FB.ftl",
            env!("CARGO_MANIFEST_DIR")
        );
        let pathbuf = PathBuf::from(path_str);

        let config = I18nConfig::new(LANG_A)
            .try_with_auto_locales(pathbuf.clone())
            .ok()
            .unwrap();

        assert_eq!(config.locales.len(), 1);
        assert!(config.locales.get(&langid!("fb-FB")).is_some());

        assert_eq!(config.locale_resources.len(), 1);
        assert!(config
            .locale_resources
            .contains(&LocaleResource::Path(pathbuf)));
    }

    #[test]
    fn will_fail_auto_locales_with_invalid_folder() {
        const LANG_A: LanguageIdentifier = langid!("la-LA");

        let root_path_str = &format!("{}/non_existing_path/", env!("CARGO_MANIFEST_DIR"));
        let pathbuf = PathBuf::from(root_path_str);

        let config = I18nConfig::new(LANG_A).try_with_auto_locales(pathbuf);
        assert_eq!(config.is_err(), true);
    }

    #[test]
    fn will_fail_auto_locales_with_invalid_file() {
        const LANG_A: LanguageIdentifier = langid!("la-LA");

        let path_str = &format!(
            "{}/tests/data/fallback/invalid_language_id.ftl",
            env!("CARGO_MANIFEST_DIR")
        );
        let pathbuf = PathBuf::from(path_str);

        let config = I18nConfig::new(LANG_A).try_with_auto_locales(pathbuf);
        assert_eq!(config.is_err(), true);
    }
}
