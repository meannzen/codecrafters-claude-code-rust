use gray_matter::Matter;
use gray_matter::engine::YAML;
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
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
    pub dir: PathBuf,
    pub body: String,
}

impl Skill {
    pub fn with_location(&self) -> String {
        format!(
            "Skill: {} (located at {})\nPaths in the instructions below are relative to that folder.\n\n{}",
            self.name,
            self.dir.display(),
            self.body.trim()
        )
    }

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
    pub fn resolve_prompt_multiple_skills(
        prompt: &str,
        skills: &[Skill],
    ) -> Result<String, String> {
        if !prompt.starts_with('/') {
            return Ok(prompt.to_string());
        }

        let mut skill_names = Vec::new();
        let mut arguments = Vec::new();

        for token in prompt.split_whitespace() {
            match token.strip_prefix('/') {
                Some(name) => skill_names.push(name),
                None => arguments.push(token.to_string()),
            }
        }

        skill_names
            .iter()
            .map(|name| {
                skills
                    .iter()
                    .find(|s| s.name == *name)
                    .cloned()
                    .map(|mut s| s.add_arguments(arguments.clone()).with_location())
                    .ok_or_else(|| format!("Unknown skill: /{name}"))
            })
            .collect::<Result<Vec<String>, String>>()
            .map(|bodies| bodies.join("\n\n"))
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
                        dir: path.parent().unwrap_or(Path::new("")).to_path_buf(),
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
            dir: PathBuf::from(".claude/skills/test"),
            body: body.to_string(),
        }
    }

    fn located(name: &str, body: &str) -> String {
        format!(
            "Skill: {name} (located at .claude/skills/{name})\nPaths in the instructions below are relative to that folder.\n\n{body}"
        )
    }

    fn skill_with_name_body(name: &str, body: &str) -> Skill {
        Skill {
            name: name.to_string(),
            description: "test skill".to_string(),
            dir: PathBuf::from(format!(".claude/skills/{name}")),
            body: body.to_string(),
        }
    }

    #[test]
    fn replaces_positional_and_arguments_placeholders() {
        let mut skill = skill_with_body("Deploy to $0 in region $1. Full request was: $ARGUMENTS");

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

        assert_eq!(result.body, "Deploy to $0. Full request was: ");
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
        let result = SkillParser::resolve_prompt_multiple_skills("hello there", &[]);

        assert_eq!(result, Ok("hello there".to_string()));
    }

    #[test]
    fn resolve_prompt_resolves_skill_with_arguments() {
        let skills = vec![skill_with_name_body(
            "deploy",
            "Deploy to $0. Full request was: $ARGUMENTS",
        )];

        let result = SkillParser::resolve_prompt_multiple_skills("/deploy prod", &skills);

        assert_eq!(
            result,
            Ok(located("deploy", "Deploy to prod. Full request was: prod"))
        );
    }

    #[test]
    fn resolve_prompt_errors_on_unknown_skill() {
        let result = SkillParser::resolve_prompt_multiple_skills("/missing", &[]);

        assert_eq!(result, Err("Unknown skill: /missing".to_string()));
    }

    #[test]
    fn resolve_prompt_multiple_skills_resolves_each_segment() {
        let skills = vec![
            skill_with_name_body("apple", "apple qty: $ARGUMENTS"),
            skill_with_name_body("fish", "fish is small"),
        ];

        let result = SkillParser::resolve_prompt_multiple_skills("/apple 11 /fish", &skills);

        assert_eq!(result, Ok(format!(
                "{}\n\n{}",
                located("apple", "apple qty: 11"),
                located("fish", "fish is small")
            )));
    }

    #[test]
    fn resolve_prompt_multiple_skills_shares_trailing_argument() {
        let skills = vec![
            skill_with_name_body("lumen", "nectarine-$ARGUMENTS"),
            skill_with_name_body("falcon", "kumquat-$ARGUMENTS"),
        ];

        let result = SkillParser::resolve_prompt_multiple_skills("/lumen /falcon 7781", &skills);

        assert_eq!(result, Ok(format!(
                "{}\n\n{}",
                located("lumen", "nectarine-7781"),
                located("falcon", "kumquat-7781")
            )));
    }
}
