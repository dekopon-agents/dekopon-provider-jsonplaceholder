//! Pure clap proposals for the `placeholder` word; stdin is consumed only by authorized invoke.
use crate::{COMMAND_WORD, CreatePost, GetPost, JsonPlaceholder};
use dekopon_provider_sdk::clap::{Args, Parser, Subcommand};
use dekopon_provider_sdk::provider::{Proposal, Usage};
use serde_json::json;

#[derive(Parser)]
#[command(name = COMMAND_WORD, version, about = "Read and create JSONPlaceholder posts")]
pub struct Placeholder {
    #[command(subcommand)]
    resource: Resource,
}
#[derive(Subcommand)]
enum Resource {
    /// Read and create posts
    #[command(subcommand)]
    Posts(Posts),
}
#[derive(Subcommand)]
enum Posts {
    /// Get one post by ID
    Get(Get),
    /// Create one post; JSONPlaceholder echoes it back and does not persist it
    Create(Create),
}
#[derive(Args)]
struct Get {
    /// The post to read, 1 to 100
    #[arg(long, value_name = "ID")]
    post_id: u32,
}
#[derive(Args)]
struct Create {
    /// The author, 1 to 10
    #[arg(long, value_name = "ID")]
    user_id: u32,
    /// The title, up to 256 UTF-8 bytes
    #[arg(long, value_name = "TEXT")]
    title: String,
    /// The body, up to 4096 UTF-8 bytes. `-` reads the piped value
    #[arg(long, value_name = "TEXT")]
    body: String,
}

pub(crate) fn propose(
    args: Placeholder,
    stdin_piped: bool,
) -> Result<Proposal<JsonPlaceholder>, Usage> {
    match args.resource {
        Resource::Posts(Posts::Get(get)) => Ok(Proposal::to::<GetPost>(
            serde_json::from_value(json!({"postId": get.post_id})).expect("clap get input"),
        )),
        Resource::Posts(Posts::Create(create)) => {
            let piped = create.body == "-";
            if piped && !stdin_piped {
                return Err(Usage::new(
                    "placeholder posts create --body -: nothing was piped in",
                ));
            }
            Ok(Proposal::to::<CreatePost>(
                serde_json::from_value(json!({
                    "userId": create.user_id,
                    "title": create.title,
                    "body": &create.body,
                    "stdinPiped": piped,
                }))
                .expect("clap create input"),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::JsonPlaceholder;
    use dekopon_provider_sdk::{CommandRunOutcome, provider};
    use serde_json::json;

    fn command(words: &[&str], piped: bool) -> CommandRunOutcome {
        provider::command::<JsonPlaceholder>(
            &words.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>(),
            piped,
        )
    }

    #[test]
    fn piped_proposal_contains_only_marker_not_private_bytes() {
        let CommandRunOutcome::Proposed {
            capability, input, ..
        } = command(
            &[
                "posts",
                "create",
                "--user-id",
                "1",
                "--title",
                "t",
                "--body",
                "-",
            ],
            true,
        )
        else {
            panic!("expected proposal")
        };
        assert_eq!(capability.as_str(), "jsonplaceholder.posts.create");
        assert_eq!(
            input,
            json!({"userId": 1, "title": "t", "body": "-", "stdinPiped": true})
        );
        assert!(!input.to_string().contains("private-piped-canary"));
        assert!(matches!(
            command(
                &[
                    "posts",
                    "create",
                    "--user-id",
                    "1",
                    "--title",
                    "t",
                    "--body",
                    "-"
                ],
                false
            ),
            CommandRunOutcome::Failed { .. }
        ));
    }

    #[test]
    fn get_and_create_propose_distinct_authority() {
        let CommandRunOutcome::Proposed {
            capability, input, ..
        } = command(&["posts", "get", "--post-id", "7"], false)
        else {
            panic!("GET proposal")
        };
        assert_eq!(capability.as_str(), "jsonplaceholder.posts.get");
        assert_eq!(input, json!({"postId": 7}));
        let CommandRunOutcome::Proposed {
            capability, input, ..
        } = command(
            &[
                "posts",
                "create",
                "--user-id",
                "3",
                "--title",
                "t",
                "--body",
                "b",
            ],
            false,
        )
        else {
            panic!("create proposal")
        };
        assert_eq!(capability.as_str(), "jsonplaceholder.posts.create");
        assert_eq!(input, json!({"userId": 3, "title": "t", "body": "b"}));
    }

    #[test]
    fn help_and_usage_render_without_authority() {
        let CommandRunOutcome::Rendered {
            stdout,
            stderr,
            status,
        } = command(&["--help"], false)
        else {
            panic!("help")
        };
        assert_eq!(status, 0);
        assert!(stdout.contains("Usage: placeholder <COMMAND>"));
        assert!(stderr.is_empty());
        let CommandRunOutcome::Rendered {
            stdout,
            stderr,
            status,
        } = command(&["posts", "get"], false)
        else {
            panic!("usage")
        };
        assert_eq!(status, 2);
        assert!(stdout.is_empty());
        assert!(stderr.contains("--post-id"));
    }
}
