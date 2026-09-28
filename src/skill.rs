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
    pub body: String,
}

impl Skill {
    pub fn add_arguments(&mut self, arguments: Vec<String>) -> Self {
        self.body = self.body.replace("$ARGUMENTS", &arguments.join(" "));

        for (index, argument) in arguments.iter().enumerate().rev() {
            self.body = self.body.replace(&format!("${index}"), argument);
        }

        self.clone()
    }
}

pub struct SkillParser;

impl SkillParser {
    pub fn resolve_prompt(prompt: &str, skills: &[Skill]) -> Result<String, String> {
        let Some(rest) = prompt.strip_prefix('/') else {
            return Ok(prompt.to_string());
        };

        let mut parts = rest.split_whitespace();
        let skill_name = parts.next().unwrap_or_default();
        let arguments: Vec<String> = parts.map(String::from).collect();

        skills
            .iter()
            .find(|s| s.name == skill_name)
            .cloned()
            .map(|mut s| s.add_arguments(arguments).body)
            .ok_or_else(|| format!("Unknown skill: /{skill_name}"))
    }

    pub fn parse<P: AsRef<Path>>(skills_root: P) -> Result<Vec<Skill>, Box<dyn std::error::Error>> {
        let mut skills = Vec::new();

        for entry in WalkDir::new(skills_root).into_iter().filter_map(Result::ok) {
            let path = entry.path();

            if path.is_file() && path.file_name().is_some_and(|name| name == "SKILL.md") {
                let file_content = fs::read_to_string(path)?;

                let matter = Matter::<YAML>::new();
                let parsed_file = matter.parse(&file_content);

                if let Some(data) = parsed_file.data
                    && let Ok(metadata) = data.deserialize::<SkillMetadata>()
                {
                    skills.push(Skill {
                        name: metadata.name,
                        description: metadata.description,
                        body: parsed_file.content,
                    });
                }
            }
        }

        Ok(skills)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skill_with_body(body: &str) -> Skill {
        Skill {
            name: "test".to_string(),
            description: "test skill".to_string(),
            body: body.to_string(),
        }
    }

    #[test]
    fn replaces_positional_and_arguments_placeholders() {
        let mut skill =
            skill_with_body("Deploy to $0 in region $1. Full request was: $ARGUMENTS");

        let result = skill.add_arguments(vec!["prod".to_string(), "us-east-1".to_string()]);

        assert_eq!(
            result.body,
            "Deploy to prod in region us-east-1. Full request was: prod us-east-1"
        );
        assert_eq!(skill.body, result.body);
    }

    #[test]
    fn leaves_body_unchanged_when_no_arguments() {
        let mut skill = skill_with_body("Deploy to $0. Full request was: $ARGUMENTS");

        let result = skill.add_arguments(vec![]);

        assert_eq!(
            result.body,
            "Deploy to $0. Full request was: "
        );
    }

    #[test]
    fn handles_double_digit_indexes_before_single_digit() {
        let mut skill = skill_with_body("first=$1 tenth=$10");
        let arguments: Vec<String> = (0..=10).map(|i| format!("arg{i}")).collect();

        let result = skill.add_arguments(arguments);

        assert_eq!(result.body, "first=arg1 tenth=arg10");
    }

    #[test]
    fn resolve_prompt_passes_through_plain_text() {
        let result = SkillParser::resolve_prompt("hello there", &[]);

        assert_eq!(result, Ok("hello there".to_string()));
    }

    #[test]
    fn resolve_prompt_resolves_skill_with_arguments() {
        let skills = vec![skill_with_body("Deploy to $0. Full request was: $ARGUMENTS")];
        let mut named = skills;
        named[0].name = "deploy".to_string();

        let result = SkillParser::resolve_prompt("/deploy prod", &named);

        assert_eq!(
            result,
            Ok("Deploy to prod. Full request was: prod".to_string())
        );
    }

    #[test]
    fn resolve_prompt_errors_on_unknown_skill() {
        let result = SkillParser::resolve_prompt("/missing", &[]);

        assert_eq!(result, Err("Unknown skill: /missing".to_string()));
    }
}
