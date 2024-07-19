use std::borrow::Cow;
use std::fs;
use std::path::PathBuf;
use std::str::from_utf8;

use log::error;
use wgpu::core::pipeline::{CreateShaderModuleError, ShaderModuleSource};
use wgpu::naga::error::ShaderError;
use wgpu::Features;
use wgpu::{naga, Limits};

pub struct Reloader {
    path: PathBuf,
    contents: Option<Vec<u8>>,
    has_changed: bool,
}

impl Reloader {
    pub fn new(rust_file: &str, data_file: &str) -> Self {
        let mut path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push(rust_file);
        path.pop();
        path.push(data_file);
        Self {
            path,
            contents: None,
            has_changed: true,
        }
    }

    pub fn get_changed_content(&mut self) -> Option<Option<&[u8]>> {
        let new_contents = fs::read(&self.path)
            .map_err(|e| {
                // only log this once
                if self.contents.is_some() || self.has_changed {
                    error!("Could not read file {}: {}", self.path.to_string_lossy(), e)
                }
            })
            .ok();
        if new_contents != self.contents {
            self.contents = new_contents;
            self.has_changed = true;
        }
        if self.has_changed {
            self.has_changed = false;
            Some(self.contents.as_ref().map(|it| it.as_ref()))
        } else {
            None
        }
    }
}

pub fn validate_shader<'a>(
    file_contents: Option<&'a [u8]>,
    features: &Features,
    limits: &Limits,
    label: &str,
    entry_points: &[&str],
) -> Result<&'a str, Cow<'static, str>> {
    if let Some(bytes) = file_contents {
        let source = from_utf8(bytes).map_err(|_| Cow::Borrowed("File is not valid utf8"))?;
        let module_source = ShaderModuleSource::Wgsl(Cow::Borrowed(&source));
        match validate_shader_module(features, limits, label, module_source) {
            Ok(parsed_entry_points) => {
                let mut message = String::new();
                for missing in entry_points
                    .iter()
                    .filter(|it| !parsed_entry_points.iter().any(|e| e == *it))
                {
                    if !message.is_empty() {
                        message += ", ";
                    }
                    message += missing;
                }

                // during modifications sometimes an empty file was read,
                // which passed validation but didn't have the entry points
                if message.is_empty() {
                    Ok(source)
                } else {
                    Err(Cow::Owned(format!("Missing entry points: {message}")))
                }
            }
            Err(e) => Err(Cow::Owned(format!("Shader validation failed: {e}"))),
        }
    } else {
        Err(Cow::Borrowed("Could not read file"))
    }
}

/// I couldn't find a fallible way to create a shader module.
/// Instead of catching panics, this is a best-effort validation for hot-reloading.
/// It should detect most parse and validation errors but it doesn't generate
/// backend-specific code and it assumes "downlevel" flags are true.
///
/// The code was adapted from wgpu-core-0.18.0/src/device/resource.rs:1218
fn validate_shader_module(
    features: &Features,
    limits: &Limits,
    label: &str,
    source: ShaderModuleSource,
) -> Result<Vec<String>, CreateShaderModuleError> {
    let (module, source) = match source {
        ShaderModuleSource::Wgsl(code) => {
            let module = naga::front::wgsl::parse_str(&code).map_err(|inner| {
                CreateShaderModuleError::Parsing(ShaderError {
                    source: code.to_string(),
                    label: Some(label.to_string()),
                    inner: Box::new(inner),
                })
            })?;
            (Cow::Owned(module), code.into_owned())
        }
        ShaderModuleSource::Naga(module) => (module, String::new()),
        ShaderModuleSource::Dummy(_) => panic!("found `ShaderModuleSource::Dummy`"),
    };
    for (_, var) in module.global_variables.iter() {
        match var.binding {
            Some(ref br) if br.group >= limits.max_bind_groups => {
                return Err(CreateShaderModuleError::InvalidGroupIndex {
                    bind: br.clone(),
                    group: br.group,
                    limit: limits.max_bind_groups,
                });
            }
            _ => continue,
        };
    }

    use naga::valid::Capabilities as Caps;

    let mut caps = Caps::empty();
    let mut feature = |c, f| caps.set(c, features.contains(f));

    feature(Caps::PUSH_CONSTANT, Features::PUSH_CONSTANTS);
    feature(Caps::FLOAT64, Features::SHADER_F64);
    feature(Caps::PRIMITIVE_INDEX, Features::SHADER_PRIMITIVE_INDEX);
    feature(
        Caps::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING,
        Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING,
    );
    feature(
        Caps::UNIFORM_BUFFER_AND_STORAGE_TEXTURE_ARRAY_NON_UNIFORM_INDEXING,
        Features::UNIFORM_BUFFER_AND_STORAGE_TEXTURE_ARRAY_NON_UNIFORM_INDEXING,
    );
    feature(
        Caps::SAMPLER_NON_UNIFORM_INDEXING,
        Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING,
    );
    feature(
        Caps::STORAGE_TEXTURE_16BIT_NORM_FORMATS,
        Features::TEXTURE_FORMAT_16BIT_NORM,
    );
    feature(Caps::MULTIVIEW, Features::MULTIVIEW);
    feature(Caps::EARLY_DEPTH_TEST, Features::SHADER_EARLY_DEPTH_TEST);
    feature(Caps::DUAL_SOURCE_BLENDING, Features::DUAL_SOURCE_BLENDING);

    // No idea how to get downlevel flags, so just assume they are available
    caps.set(Caps::MULTISAMPLED_SHADING, true);
    caps.set(Caps::CUBE_ARRAY_TEXTURES, true);

    naga::valid::Validator::new(naga::valid::ValidationFlags::all(), caps)
        .validate(&module)
        .map_err(|inner| {
            CreateShaderModuleError::Validation(ShaderError {
                source,
                label: Some(label.to_string()),
                inner: Box::new(inner),
            })
        })?;

    Ok(module
        .entry_points
        .iter()
        .map(|it| it.name.clone())
        .collect())
}
