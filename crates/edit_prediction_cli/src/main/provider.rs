use super::*;

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TeacherBackend {
    Sonnet46,
    #[default]
    Sonnet45,
    Gpt52,
    Gpt54,
    Gpt55,
}

impl std::fmt::Display for TeacherBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TeacherBackend::Sonnet46 => write!(f, "sonnet46"),
            TeacherBackend::Sonnet45 => write!(f, "sonnet45"),
            TeacherBackend::Gpt52 => write!(f, "gpt52"),
            TeacherBackend::Gpt54 => write!(f, "gpt54"),
            TeacherBackend::Gpt55 => write!(f, "gpt55"),
        }
    }
}

impl std::str::FromStr for TeacherBackend {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "sonnet45" | "sonnet" | "claude" => Ok(TeacherBackend::Sonnet45),
            "sonnet46" => Ok(TeacherBackend::Sonnet46),
            "gpt52" => Ok(TeacherBackend::Gpt52),
            "gpt54" | "gpt" | "openai" => Ok(TeacherBackend::Gpt54),
            "gpt55" => Ok(TeacherBackend::Gpt55),
            "v0114180editableregion" => Ok(TeacherBackend::Sonnet45),
            _ => anyhow::bail!(
                "unknown teacher backend `{s}`. Valid options: sonnet45, sonnet46, gpt52, gpt54, gpt55"
            ),
        }
    }
}

impl TeacherBackend {
    pub fn model_name(&self) -> &'static str {
        match self {
            TeacherBackend::Sonnet45 => "claude-sonnet-4-5",
            TeacherBackend::Sonnet46 => "claude-sonnet-4-6",
            TeacherBackend::Gpt52 => "gpt-5.2",
            TeacherBackend::Gpt54 => "gpt-5.4",
            TeacherBackend::Gpt55 => "gpt-5.5",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum PredictionProvider {
    Mercury,
    Zeta1,
    Zeta2(ZetaFormat),
    Baseten(ZetaFormat),
    Teacher(TeacherBackend, ZetaFormat),
    TeacherJumps(TeacherBackend),
    TeacherNonBatching(TeacherBackend, ZetaFormat),
    TeacherJumpsNonBatching(TeacherBackend),
    Repair,
}

impl Default for PredictionProvider {
    fn default() -> Self {
        PredictionProvider::Zeta2(ZetaFormat::default())
    }
}

impl std::fmt::Display for PredictionProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PredictionProvider::Mercury => write!(f, "mercury"),
            PredictionProvider::Zeta1 => write!(f, "zeta1"),
            PredictionProvider::Zeta2(format) => write!(f, "zeta2:{format}"),
            PredictionProvider::Baseten(format) => write!(f, "baseten:{format}"),
            PredictionProvider::Teacher(backend, format) => {
                write!(f, "teacher:{backend}:{format:?}")
            }
            PredictionProvider::TeacherJumps(backend) => {
                write!(f, "teacher-jumps:{backend}")
            }
            PredictionProvider::TeacherNonBatching(backend, format) => {
                write!(f, "teacher-non-batching:{backend}:{format:?}")
            }
            PredictionProvider::TeacherJumpsNonBatching(backend) => {
                write!(f, "teacher-jumps-non-batching:{backend}")
            }
            PredictionProvider::Repair => write!(f, "repair"),
        }
    }
}

impl std::str::FromStr for PredictionProvider {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (provider, arg) = s.split_once(':').map_or((s, None), |(p, a)| (p, Some(a)));

        let provider_lower = provider.to_lowercase();
        match provider_lower.as_str() {
            "mercury" => Ok(PredictionProvider::Mercury),
            "zeta1" => Ok(PredictionProvider::Zeta1),
            "zeta2" => {
                let format = arg.map(ZetaFormat::parse).transpose()?.unwrap_or_default();
                Ok(PredictionProvider::Zeta2(format))
            }
            "teacher" => {
                let (backend, format) = parse_teacher_args(arg)?;
                Ok(PredictionProvider::Teacher(backend, format))
            }
            "teacher-non-batching" | "teacher_non_batching" => {
                let (backend, format) = parse_teacher_args(arg)?;
                Ok(PredictionProvider::TeacherNonBatching(backend, format))
            }
            "teacher-jumps" | "teacher_jumps" => {
                let backend = arg
                    .map(|a| a.parse())
                    .transpose()?
                    .unwrap_or(TeacherBackend::default());
                Ok(PredictionProvider::TeacherJumps(backend))
            }
            "teacher-jumps-non-batching" | "teacher_jumps_non_batching" => {
                let backend = arg
                    .map(|a| a.parse())
                    .transpose()?
                    .unwrap_or(TeacherBackend::default());
                Ok(PredictionProvider::TeacherJumpsNonBatching(backend))
            }
            "repair" => Ok(PredictionProvider::Repair),
            "baseten" => {
                let format = arg
                    .map(ZetaFormat::parse)
                    .transpose()?
                    .unwrap_or(ZetaFormat::default());
                Ok(PredictionProvider::Baseten(format))
            }
            _ => {
                anyhow::bail!(
                    "unknown provider `{provider}`. Valid options: mercury, zeta1, zeta2, zeta2:<version>, teacher, teacher:<backend>, teacher-jumps, teacher-jumps:<backend>, teacher-non-batching, teacher-jumps-non-batching, repair\n\
                 For zeta2, you can optionally specify a version like `zeta2:ordered` or `zeta2:V0113_Ordered`.\n\
                 For teacher providers, you can specify a backend like `teacher:sonnet46`, `teacher-jumps:sonnet46`, `teacher-jumps-non-batching:sonnet46`, or `teacher:gpt52`.\n\
                 Available zeta versions:\n{}",
                    ZetaFormat::options_as_string()
                )
            }
        }
    }
}

fn parse_teacher_args(arg: Option<&str>) -> Result<(TeacherBackend, ZetaFormat), anyhow::Error> {
    let mut backend = TeacherBackend::default();
    let mut format = ZetaFormat::default();

    for arg in arg.unwrap_or_default().split(':') {
        if arg.is_empty() {
            continue;
        }

        if let Ok(parsed_backend) = TeacherBackend::from_str(arg) {
            backend = parsed_backend;
        } else if let Ok(parsed_format) = ZetaFormat::parse(arg) {
            format = parsed_format;
        } else {
            anyhow::bail!("unknown teacher backend or zeta format `{arg}`");
        }
    }

    Ok((backend, format))
}

impl Serialize for PredictionProvider {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for PredictionProvider {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prediction_provider_jumps_non_batched_round_trips_to_primary_spelling() {
        let provider: PredictionProvider = "teacher-jumps-non-batching:sonnet46".parse().unwrap();
        assert_eq!(
            provider,
            PredictionProvider::TeacherJumpsNonBatching(TeacherBackend::Sonnet46)
        );
        assert_eq!(provider.to_string(), "teacher-jumps-non-batching:sonnet46");
    }

    #[test]
    fn prediction_provider_jumps_non_batched_alias_round_trips_to_primary_spelling() {
        let provider: PredictionProvider = "teacher_jumps_non_batching:gpt52".parse().unwrap();
        assert_eq!(
            provider,
            PredictionProvider::TeacherJumpsNonBatching(TeacherBackend::Gpt52)
        );
        assert_eq!(provider.to_string(), "teacher-jumps-non-batching:gpt52");
    }
}
