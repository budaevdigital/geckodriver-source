use crate::logging;
use base64;
use hyper::Method;
use serde::de::{self, Deserialize, Deserializer};
use serde_json::{self, Value};
use std::env;
use std::fs::File;
use std::io::prelude::*;
use uuid::Uuid;
use webdriver::command::{WebDriverCommand, WebDriverExtensionCommand};
use webdriver::common::WebElement;
use webdriver::error::{ErrorStatus, WebDriverError, WebDriverResult};
use webdriver::httpapi::WebDriverExtensionRoute;
use webdriver::Parameters;

pub const CHROME_ELEMENT_KEY: &str = "chromeelement-9fc5-4b51-a3c8-01716eedeb04";

pub fn extension_routes() -> Vec<(Method, &'static str, GeckoExtensionRoute)> {
    return vec![
        (
            Method::GET,
            "/session/{sessionId}/moz/context",
            GeckoExtensionRoute::GetContext,
        ),
        (
            Method::POST,
            "/session/{sessionId}/moz/context",
            GeckoExtensionRoute::SetContext,
        ),
        (
            Method::POST,
            "/session/{sessionId}/moz/xbl/{elementId}/anonymous_children",
            GeckoExtensionRoute::XblAnonymousChildren,
        ),
        (
            Method::POST,
            "/session/{sessionId}/moz/xbl/{elementId}/anonymous_by_attribute",
            GeckoExtensionRoute::XblAnonymousByAttribute,
        ),
        (
            Method::POST,
            "/session/{sessionId}/moz/addon/install",
            GeckoExtensionRoute::InstallAddon,
        ),
        (
            Method::POST,
            "/session/{sessionId}/moz/addon/uninstall",
            GeckoExtensionRoute::UninstallAddon,
        ),
        (
            Method::GET,
            "/session/{sessionId}/moz/screenshot/full",
            GeckoExtensionRoute::TakeFullScreenshot,
        ),
        (
            Method::POST,
            "/session/{sessionId}/moz/print",
            GeckoExtensionRoute::Print,
        ),
    ];
}

#[derive(Clone, PartialEq)]
pub enum GeckoExtensionRoute {
    GetContext,
    SetContext,
    XblAnonymousChildren,
    XblAnonymousByAttribute,
    InstallAddon,
    UninstallAddon,
    TakeFullScreenshot,
    Print,
}

impl WebDriverExtensionRoute for GeckoExtensionRoute {
    type Command = GeckoExtensionCommand;

    fn command(
        &self,
        params: &Parameters,
        body_data: &Value,
    ) -> WebDriverResult<WebDriverCommand<GeckoExtensionCommand>> {
        use self::GeckoExtensionRoute::*;

        let command = match *self {
            GetContext => GeckoExtensionCommand::GetContext,
            SetContext => {
                GeckoExtensionCommand::SetContext(serde_json::from_value(body_data.clone())?)
            }
            XblAnonymousChildren => {
                let element_id = try_opt!(
                    params.get("elementId"),
                    ErrorStatus::InvalidArgument,
                    "Missing elementId parameter"
                );
                let element = WebElement(element_id.as_str().to_string());
                GeckoExtensionCommand::XblAnonymousChildren(element)
            }
            XblAnonymousByAttribute => {
                let element_id = try_opt!(
                    params.get("elementId"),
                    ErrorStatus::InvalidArgument,
                    "Missing elementId parameter"
                );
                GeckoExtensionCommand::XblAnonymousByAttribute(
                    WebElement(element_id.as_str().into()),
                    serde_json::from_value(body_data.clone())?,
                )
            }
            InstallAddon => {
                GeckoExtensionCommand::InstallAddon(serde_json::from_value(body_data.clone())?)
            }
            UninstallAddon => {
                GeckoExtensionCommand::UninstallAddon(serde_json::from_value(body_data.clone())?)
            }
            TakeFullScreenshot => GeckoExtensionCommand::TakeFullScreenshot,
            Print => GeckoExtensionCommand::Print(serde_json::from_value(body_data.clone())?),
        };

        Ok(WebDriverCommand::Extension(command))
    }
}

#[derive(Clone, PartialEq)]
pub enum GeckoExtensionCommand {
    GetContext,
    SetContext(GeckoContextParameters),
    XblAnonymousChildren(WebElement),
    XblAnonymousByAttribute(WebElement, XblLocatorParameters),
    InstallAddon(AddonInstallParameters),
    UninstallAddon(AddonUninstallParameters),
    TakeFullScreenshot,
    Print(PrintParameters),
}

impl WebDriverExtensionCommand for GeckoExtensionCommand {
    fn parameters_json(&self) -> Option<Value> {
        use self::GeckoExtensionCommand::*;
        match self {
            GetContext => None,
            InstallAddon(x) => Some(serde_json::to_value(x).unwrap()),
            SetContext(x) => Some(serde_json::to_value(x).unwrap()),
            UninstallAddon(x) => Some(serde_json::to_value(x).unwrap()),
            XblAnonymousByAttribute(_, x) => Some(serde_json::to_value(x).unwrap()),
            XblAnonymousChildren(_) => None,
            TakeFullScreenshot => None,
            Print(x) => Some(serde_json::to_value(x).unwrap()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AddonInstallParameters {
    pub path: String,
    pub temporary: Option<bool>,
}

impl<'de> Deserialize<'de> for AddonInstallParameters {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Base64 {
            addon: String,
            temporary: Option<bool>,
        };

        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Path {
            path: String,
            temporary: Option<bool>,
        };

        #[derive(Debug, Deserialize)]
        #[serde(untagged)]
        enum Helper {
            Base64(Base64),
            Path(Path),
        };

        let params = match Helper::deserialize(deserializer)? {
            Helper::Path(ref mut data) => AddonInstallParameters {
                path: data.path.clone(),
                temporary: data.temporary,
            },
            Helper::Base64(ref mut data) => {
                let content = base64::decode(&data.addon).map_err(de::Error::custom)?;

                let path = env::temp_dir()
                    .as_path()
                    .join(format!("addon-{}.xpi", Uuid::new_v4()));
                let mut xpi_file = File::create(&path).map_err(de::Error::custom)?;
                xpi_file
                    .write(content.as_slice())
                    .map_err(de::Error::custom)?;

                let path = match path.to_str() {
                    Some(path) => path.to_string(),
                    None => return Err(de::Error::custom("could not write addon to file")),
                };

                AddonInstallParameters {
                    path,
                    temporary: data.temporary,
                }
            }
        };

        Ok(params)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AddonUninstallParameters {
    pub id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GeckoContext {
    Content,
    Chrome,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GeckoContextParameters {
    pub context: GeckoContext,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct XblLocatorParameters {
    pub name: String,
    pub value: String,
}

#[derive(Default, Debug, PartialEq)]
pub struct LogOptions {
    pub level: Option<logging::Level>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PrintParameters {
    pub orientation: PrintOrientation,
    #[serde(deserialize_with = "deserialize_to_print_scale_f64")]
    pub scale: f64,
    pub background: bool,
    pub page: PrintPage,
    pub margin: PrintMargins,
    pub page_ranges: Vec<String>,
    pub shrink_to_fit: bool,
}

impl Default for PrintParameters {
    fn default() -> Self {
        PrintParameters {
            orientation: PrintOrientation::default(),
            scale: 1.0,
            background: false,
            page: PrintPage::default(),
            margin: PrintMargins::default(),
            page_ranges: Vec::new(),
            shrink_to_fit: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PrintOrientation {
    Landscape,
    Portrait,
}

impl Default for PrintOrientation {
    fn default() -> Self {
        PrintOrientation::Portrait
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PrintPage {
    #[serde(deserialize_with = "deserialize_to_positive_f64")]
    pub width: f64,
    #[serde(deserialize_with = "deserialize_to_positive_f64")]
    pub height: f64,
}

impl Default for PrintPage {
    fn default() -> Self {
        PrintPage {
            width: 21.59,
            height: 27.94,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PrintMargins {
    #[serde(deserialize_with = "deserialize_to_positive_f64")]
    pub top: f64,
    #[serde(deserialize_with = "deserialize_to_positive_f64")]
    pub bottom: f64,
    #[serde(deserialize_with = "deserialize_to_positive_f64")]
    pub left: f64,
    #[serde(deserialize_with = "deserialize_to_positive_f64")]
    pub right: f64,
}

impl Default for PrintMargins {
    fn default() -> Self {
        PrintMargins {
            top: 1.0,
            bottom: 1.0,
            left: 1.0,
            right: 1.0,
        }
    }
}

fn deserialize_to_positive_f64<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    let val = f64::deserialize(deserializer)?;
    if val < 0.0 {
        return Err(de::Error::custom(format!("{} is negative", val)));
    };
    Ok(val)
}

fn deserialize_to_print_scale_f64<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    let val = f64::deserialize(deserializer)?;
    if val < 0.1 || val > 2.0 {
        return Err(de::Error::custom(format!("{} is outside range 0.1-2", val)));
    };
    Ok(val)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::test::assert_de;

    #[test]
    fn test_json_addon_install_parameters_invalid() {
        assert!(serde_json::from_str::<AddonInstallParameters>("").is_err());
        assert!(serde_json::from_value::<AddonInstallParameters>(json!(null)).is_err());
        assert!(serde_json::from_value::<AddonInstallParameters>(json!({})).is_err());
    }

    #[test]
    fn test_json_addon_install_parameters_with_path_and_temporary() {
        let params = AddonInstallParameters {
            path: "/path/to.xpi".to_string(),
            temporary: Some(true),
        };
        assert_de(&params, json!({"path": "/path/to.xpi", "temporary": true}));
    }

    #[test]
    fn test_json_addon_install_parameters_with_path() {
        let params = AddonInstallParameters {
            path: "/path/to.xpi".to_string(),
            temporary: None,
        };
        assert_de(&params, json!({"path": "/path/to.xpi"}));
    }

    #[test]
    fn test_json_addon_install_parameters_with_path_invalid_type() {
        let json = json!({"path": true, "temporary": true});
        assert!(serde_json::from_value::<AddonInstallParameters>(json).is_err());
    }

    #[test]
    fn test_json_addon_install_parameters_with_path_and_temporary_invalid_type() {
        let json = json!({"path": "/path/to.xpi", "temporary": "foo"});
        assert!(serde_json::from_value::<AddonInstallParameters>(json).is_err());
    }

    #[test]
    fn test_json_addon_install_parameters_with_addon() {
        let json = json!({"addon": "aGVsbG8=", "temporary": true});
        let data = serde_json::from_value::<AddonInstallParameters>(json).unwrap();

        assert_eq!(data.temporary, Some(true));
        let mut file = File::open(data.path).unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();
        assert_eq!(contents, "hello");
    }

    #[test]
    fn test_json_addon_install_parameters_with_addon_only() {
        let json = json!({"addon": "aGVsbG8="});
        let data = serde_json::from_value::<AddonInstallParameters>(json).unwrap();

        assert_eq!(data.temporary, None);
        let mut file = File::open(data.path).unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();
        assert_eq!(contents, "hello");
    }

    #[test]
    fn test_json_addon_install_parameters_with_addon_invalid_type() {
        let json = json!({"addon": true, "temporary": true});
        assert!(serde_json::from_value::<AddonInstallParameters>(json).is_err());
    }

    #[test]
    fn test_json_addon_install_parameters_with_addon_and_temporary_invalid_type() {
        let json = json!({"addon": "aGVsbG8=", "temporary": "foo"});
        assert!(serde_json::from_value::<AddonInstallParameters>(json).is_err());
    }

    #[test]
    fn test_json_install_parameters_with_temporary_only() {
        let json = json!({"temporary": true});
        assert!(serde_json::from_value::<AddonInstallParameters>(json).is_err());
    }

    #[test]
    fn test_json_addon_install_parameters_with_both_path_and_addon() {
        let json = json!({
            "path": "/path/to.xpi",
            "addon": "aGVsbG8=",
            "temporary": true,
        });
        assert!(serde_json::from_value::<AddonInstallParameters>(json).is_err());
    }

    #[test]
    fn test_json_addon_uninstall_parameters_invalid() {
        assert!(serde_json::from_str::<AddonUninstallParameters>("").is_err());
        assert!(serde_json::from_value::<AddonUninstallParameters>(json!(null)).is_err());
        assert!(serde_json::from_value::<AddonUninstallParameters>(json!({})).is_err());
    }

    #[test]
    fn test_json_addon_uninstall_parameters() {
        let params = AddonUninstallParameters {
            id: "foo".to_string(),
        };
        assert_de(&params, json!({"id": "foo"}));
    }

    #[test]
    fn test_json_addon_uninstall_parameters_id_invalid_type() {
        let json = json!({"id": true});
        assert!(serde_json::from_value::<AddonUninstallParameters>(json).is_err());
    }

    #[test]
    fn test_json_gecko_context_parameters_content() {
        let params = GeckoContextParameters {
            context: GeckoContext::Content,
        };
        assert_de(&params, json!({"context": "content"}));
    }

    #[test]
    fn test_json_gecko_context_parameters_chrome() {
        let params = GeckoContextParameters {
            context: GeckoContext::Chrome,
        };
        assert_de(&params, json!({"context": "chrome"}));
    }

    #[test]
    fn test_json_gecko_context_parameters_context_invalid() {
        type P = GeckoContextParameters;
        assert!(serde_json::from_value::<P>(json!({})).is_err());
        assert!(serde_json::from_value::<P>(json!({ "context": null })).is_err());
        assert!(serde_json::from_value::<P>(json!({"context": "foo"})).is_err());
    }

    #[test]
    fn test_json_xbl_anonymous_by_attribute() {
        let locator = XblLocatorParameters {
            name: "foo".to_string(),
            value: "bar".to_string(),
        };
        assert_de(&locator, json!({"name": "foo", "value": "bar"}));
    }

    #[test]
    fn test_json_xbl_anonymous_by_attribute_with_name_invalid() {
        type P = XblLocatorParameters;
        assert!(serde_json::from_value::<P>(json!({"value": "bar"})).is_err());
        assert!(serde_json::from_value::<P>(json!({"name": null, "value": "bar"})).is_err());
        assert!(serde_json::from_value::<P>(json!({"name": "foo"})).is_err());
        assert!(serde_json::from_value::<P>(json!({"name": "foo", "value": null})).is_err());
    }

    #[test]
    fn test_json_gecko_print_defaults() {
        let params = PrintParameters::default();
        assert_de(&params, json!({}));
    }

    #[test]
    fn test_json_gecko_print() {
        let params = PrintParameters {
            orientation: PrintOrientation::Landscape,
            page: PrintPage {
                width: 10.0,
                ..Default::default()
            },
            margin: PrintMargins {
                top: 10.0,
                ..Default::default()
            },
            scale: 1.5,
            ..Default::default()
        };
        assert_de(
            &params,
            json!({"orientation": "landscape", "page": {"width": 10}, "margin": {"top": 10}, "scale": 1.5}),
        );
    }

    #[test]
    fn test_json_gecko_scale_invalid() {
        assert!(serde_json::from_value::<AddonInstallParameters>(json!({"scale": 3})).is_err());
    }
}
