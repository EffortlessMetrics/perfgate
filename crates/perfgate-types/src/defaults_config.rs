#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Default)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct DefaultsConfig {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub repeat: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub warmup: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub threshold: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub warn_factor: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub noise_threshold: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub out_dir: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub baseline_dir: Option<String>,

    /// Optional baseline discovery pattern. Supports `{bench}` placeholder.
    /// Example: `baselines/{bench}.json`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub baseline_pattern: Option<String>,

    /// Optional Handlebars template path for markdown comments.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub markdown_template: Option<String>,
}





