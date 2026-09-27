use gray_matter::Matter;
use gray_matter::engine::YAML;
use serde::Deserialize;
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

#[derive(Deserialize)]
struct SkillMetadata {
    name: String,
    description: String,
}

#[derive(Debug, Clone)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub _body: String,
}

pub struct SkillParser;

impl SkillParser {
    pub fn parse<P: AsRef<Path>>(skills_root: P) -> Result<Vec<Skill>, Box<dyn std::error::Error>> {
        let mut skills = Vec::new();

        for entry in WalkDir::new(skills_root).into_iter().filter_map(Result::ok) {
            let path = entry.path();

            if path.is_file() && path.file_name().map_or(false, |name| name == "SKILL.md") {
                let file_content = fs::read_to_string(path)?;

                let matter = Matter::<YAML>::new();
                let parsed_file = matter.parse(&file_content);

                if let Some(data) = parsed_file.data {
                    if let Ok(metadata) = data.deserialize::<SkillMetadata>() {
                        skills.push(Skill {
                            name: metadata.name,
                            description: metadata.description,
                            _body: parsed_file.content,
                        });
                    }
                }
            }
        }

        Ok(skills)
    }
}
