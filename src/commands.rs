//! The `placeholder` command word: a small command-line program, rendered by the guest.
//!
//! `placeholder --help`, `placeholder posts get --help`, `placeholder --version`, and every usage
//! error are answered here and authorize nothing — the SDK's clap layer renders them as text with
//! an exit status, the way an upstream tool's `main` would. A well-formed argv becomes a
//! *proposal*: the capability and the camelCase input a direct invocation sends, which then
//! travels constraint-set lookup and Cedar like any other. Naming a capability the caller was not
//! granted is a denial, not an escalation.
//!
//! The tree is the capability identifier with the provider prefix swapped for the word:
//! `jsonplaceholder.posts.get` is `placeholder posts get`. Each flag is the kebab-case of the wire
//! field it fills, so `--post-id` is `postId`.

use dekopon_provider_sdk::clap::{self, Args, CommandFactory, FromArgMatches, Parser, Subcommand};
use dekopon_provider_sdk::{CommandInvocation, CommandRun, ProviderError, cli};
use serde_json::{Value, json};

use crate::{COMMAND_WORD, POSTS_CREATE, POSTS_GET};

/// The value that means "read the text piped into the word".
const PIPED: &str = "-";

// The `placeholder` tree, declared once and rendered by clap. Plain comments, not doc comments:
// clap renders a doc comment as the `about` line above `Usage:`.
#[derive(Parser)]
#[command(
    name = COMMAND_WORD,
    version,
    about = "Read and create JSONPlaceholder posts"
)]
struct Placeholder {
    #[command(subcommand)]
    resource: Resource,
}

#[derive(Subcommand)]
enum Resource {
    /// Read and create posts
    #[command(subcommand)]
    Posts(Posts),
}

// Each verb proposes exactly one capability, named by the `const` the manifest declares, so a
// renamed capability is a compile error rather than an exit code discovered mid-session.
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
    /// Production JSONPlaceholder HTTPS (the default) or a literal loopback http://IP:PORT
    #[arg(long, value_name = "URL")]
    endpoint: Option<String>,
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
    /// Production JSONPlaceholder HTTPS (the default) or a literal loopback http://IP:PORT
    #[arg(long, value_name = "URL")]
    endpoint: Option<String>,
}

/// Runs one `placeholder` argv.
pub(crate) fn run(argv: &[String], stdin: Option<&str>) -> Result<CommandRun, ProviderError> {
    cli::run_command(Placeholder::command(), argv, stdin, dispatch)
}

/// Turns clap's matches into the proposal for the selected verb.
///
/// Runs only after clap accepted the argv, so what is left to decide is what clap cannot know:
/// whether anything was piped. Every semantic bound — the ID ranges, the byte limits, the endpoint
/// allowlist — is checked once, in `invoke`, against the input a direct call would send too.
fn dispatch(
    matches: clap::ArgMatches,
    stdin: Option<&str>,
) -> Result<CommandInvocation, ProviderError> {
    let placeholder = Placeholder::from_arg_matches(&matches)
        .map_err(|error| ProviderError::new("usage", error.to_string()))?;
    let (capability, mut input, endpoint) = match placeholder.resource {
        Resource::Posts(Posts::Get(get)) => {
            (POSTS_GET, json!({"postId": get.post_id}), get.endpoint)
        }
        Resource::Posts(Posts::Create(create)) => (
            POSTS_CREATE,
            json!({
                "userId": create.user_id,
                "title": create.title,
                "body": body(create.body, stdin)?,
            }),
            create.endpoint,
        ),
    };
    // Absent rather than null: the input parser defaults a missing endpoint to production.
    if let Some(endpoint) = endpoint {
        input["endpoint"] = Value::String(endpoint);
    }
    Ok(CommandInvocation {
        capability: capability.parse().expect("static capability ID"),
        input,
        secret_use: None,
    })
}

/// The body, or the piped value when the caller wrote `-`.
fn body(body: String, stdin: Option<&str>) -> Result<String, ProviderError> {
    if body != PIPED {
        return Ok(body);
    }
    stdin.map(str::to_owned).ok_or_else(|| {
        ProviderError::new(
            "usage",
            format!("{COMMAND_WORD} posts create --body -: nothing was piped in"),
        )
    })
}

#[cfg(test)]
mod tests {
    use dekopon_provider_http::Response;
    use dekopon_provider_sdk::{CommandInvocation, CommandRun, Provider};
    use serde_json::json;

    use super::run;
    use crate::{JsonPlaceholder, POSTS_CREATE, POSTS_GET, invoke_with};

    fn argv(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| (*word).to_owned()).collect()
    }

    fn rendered(words: &[&str], stdin: Option<&str>) -> (String, String, u8) {
        let run = run(&argv(words), stdin).expect("clap answers are rendered, not declined");
        let CommandRun::Rendered {
            stdout,
            stderr,
            status,
        } = run
        else {
            panic!("expected rendered text for {words:?}, got {run:?}");
        };
        (stdout, stderr, status)
    }

    fn proposal(words: &[&str], stdin: Option<&str>) -> CommandInvocation {
        match run(&argv(words), stdin).expect("a well-formed argv proposes") {
            CommandRun::Proposal(invocation) => invocation,
            other => panic!("expected a proposal for {words:?}, got {other:?}"),
        }
    }

    #[test]
    fn help_renders_byte_for_byte_on_stdout_at_status_zero() {
        for (words, page) in [
            (
                &["--help"][..],
                "Read and create JSONPlaceholder posts

Usage: placeholder <COMMAND>

Commands:
  posts  Read and create posts
  help   Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
",
            ),
            (
                &["posts", "--help"][..],
                "Read and create posts

Usage: placeholder posts <COMMAND>

Commands:
  get     Get one post by ID
  create  Create one post; JSONPlaceholder echoes it back and does not persist it
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
",
            ),
            (
                &["posts", "get", "--help"][..],
                "Get one post by ID

Usage: placeholder posts get [OPTIONS] --post-id <ID>

Options:
      --post-id <ID>    The post to read, 1 to 100
      --endpoint <URL>  Production JSONPlaceholder HTTPS (the default) or a literal loopback http://IP:PORT
  -h, --help            Print help
",
            ),
            (
                &["posts", "create", "--help"][..],
                "Create one post; JSONPlaceholder echoes it back and does not persist it

Usage: placeholder posts create [OPTIONS] --user-id <ID> --title <TEXT> --body <TEXT>

Options:
      --user-id <ID>    The author, 1 to 10
      --title <TEXT>    The title, up to 256 UTF-8 bytes
      --body <TEXT>     The body, up to 4096 UTF-8 bytes. `-` reads the piped value
      --endpoint <URL>  Production JSONPlaceholder HTTPS (the default) or a literal loopback http://IP:PORT
  -h, --help            Print help
",
            ),
        ] {
            let (stdout, stderr, status) = rendered(words, None);
            assert_eq!(status, 0, "{words:?}");
            assert_eq!(stdout, page, "{words:?}");
            assert!(stderr.is_empty(), "{words:?}: {stderr}");
        }

        let (stdout, _, status) = rendered(&["-h"], None);
        assert_eq!(status, 0);
        assert!(stdout.starts_with("Read and create JSONPlaceholder posts"));

        let (stdout, _, status) = rendered(&["--version"], None);
        assert_eq!(status, 0);
        assert_eq!(
            stdout,
            format!("placeholder {}\n", env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn usage_errors_render_on_stderr_at_status_two() {
        for (words, usage) in [
            // A bare word or resource is a usage error whose text is the help page: nothing was
            // asked, so nothing is proposed.
            (&[][..], "Usage: placeholder <COMMAND>"),
            (&["bogus"][..], "Usage: placeholder <COMMAND>"),
            (&["posts"][..], "Usage: placeholder posts <COMMAND>"),
            (&["posts", "list"][..], "Usage: placeholder posts <COMMAND>"),
            (
                &["posts", "get"][..],
                "Usage: placeholder posts get --post-id <ID>",
            ),
            (
                // A value clap refused names the flag rather than repeating the usage line.
                &["posts", "get", "--post-id", "seven"][..],
                "invalid value 'seven' for '--post-id <ID>'",
            ),
            (
                &["posts", "get", "--id", "7"][..],
                "Usage: placeholder posts get",
            ),
            (
                &["posts", "get", "--postId", "7"][..],
                "Usage: placeholder posts get",
            ),
            (
                &["posts", "create", "--title", "t", "--body", "b"][..],
                "Usage: placeholder posts create --user-id <ID>",
            ),
            (
                &["posts", "create", "--user-id", "1", "--body", "b"][..],
                "Usage: placeholder posts create --user-id <ID> --title <TEXT>",
            ),
            (
                &["posts", "create", "--user-id", "1", "--title", "t"][..],
                "Usage: placeholder posts create --user-id <ID> --title <TEXT> --body <TEXT>",
            ),
        ] {
            let (stdout, stderr, status) = rendered(words, None);
            assert_eq!(status, 2, "{words:?}");
            assert!(stdout.is_empty(), "{words:?}: {stdout}");
            assert!(stderr.contains(usage), "{words:?}: {stderr}");
        }

        for words in [
            &["bogus"][..],
            &["posts", "get"][..],
            &["posts", "get", "--post-id", "seven"][..],
            &["posts", "create", "--user-id", "1", "--title", "t"][..],
        ] {
            let (_, stderr, _) = rendered(words, None);
            assert!(stderr.starts_with("error: "), "{words:?}: {stderr}");
        }
    }

    #[test]
    fn get_flags_map_onto_the_camel_case_wire_fields() {
        let invocation = proposal(&["posts", "get", "--post-id", "7"], None);
        assert_eq!(invocation.capability.as_str(), POSTS_GET);
        assert_eq!(invocation.input, json!({"postId": 7}));

        let invocation = proposal(
            &[
                "posts",
                "get",
                "--endpoint",
                "http://127.0.0.1:43123",
                "--post-id",
                "100",
            ],
            None,
        );
        assert_eq!(invocation.capability.as_str(), POSTS_GET);
        assert_eq!(
            invocation.input,
            json!({"postId": 100, "endpoint": "http://127.0.0.1:43123"})
        );
    }

    #[test]
    fn create_flags_map_onto_the_camel_case_wire_fields() {
        let invocation = proposal(
            &[
                "posts",
                "create",
                "--user-id",
                "3",
                "--title",
                "created title",
                "--body",
                "created body",
            ],
            None,
        );
        assert_eq!(invocation.capability.as_str(), POSTS_CREATE);
        assert_eq!(
            invocation.input,
            json!({"userId": 3, "title": "created title", "body": "created body"})
        );

        let invocation = proposal(
            &[
                "posts",
                "create",
                "--user-id",
                "3",
                "--title",
                "t",
                "--body",
                "b",
                "--endpoint",
                "http://[::1]:43124",
            ],
            None,
        );
        assert_eq!(
            invocation.input,
            json!({"userId": 3, "title": "t", "body": "b", "endpoint": "http://[::1]:43124"})
        );
    }

    #[test]
    fn a_dash_body_reads_the_piped_value_and_declines_when_nothing_was_piped() {
        let piped = "line one\n  indented\n";
        let invocation = proposal(
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
            Some(piped),
        );
        assert_eq!(
            invocation.input,
            json!({"userId": 1, "title": "t", "body": piped})
        );

        let error = run(
            &argv(&[
                "posts",
                "create",
                "--user-id",
                "1",
                "--title",
                "t",
                "--body",
                "-",
            ]),
            None,
        )
        .expect_err("a decline, reported to the model as a usage error");
        assert_eq!(error.code(), "usage");
        assert_eq!(
            error.message(),
            "placeholder posts create --body -: nothing was piped in"
        );
    }

    /// Bounds are `invoke`'s alone: an out-of-range ID is a well-formed argv, and the proposal is
    /// refused by the same parser a direct call meets, before any HTTP.
    #[test]
    fn every_proposal_is_input_the_invoke_parser_accepts_or_bounds_check_refuses() {
        for (words, stdin, status, echoed) in [
            (
                &["posts", "get", "--post-id", "7"][..],
                None,
                200,
                json!({"userId": 1, "id": 7, "title": "t", "body": "b"}),
            ),
            (
                &[
                    "posts",
                    "get",
                    "--post-id",
                    "7",
                    "--endpoint",
                    "http://127.0.0.1:43123",
                ][..],
                None,
                200,
                json!({"userId": 1, "id": 7, "title": "t", "body": "b"}),
            ),
            (
                &[
                    "posts",
                    "create",
                    "--user-id",
                    "2",
                    "--title",
                    "t",
                    "--body",
                    "-",
                ][..],
                Some("piped"),
                201,
                json!({"userId": 2, "id": 101, "title": "t", "body": "piped"}),
            ),
        ] {
            let invocation = proposal(words, stdin);
            let output = invoke_with(&invocation.capability, invocation.input, |_| {
                Ok(Response {
                    status,
                    headers: Vec::new(),
                    body: serde_json::to_vec(&echoed).expect("fixture serializes"),
                })
            })
            .unwrap_or_else(|error| panic!("{words:?}: {} {}", error.code(), error.message()));
            assert_eq!(output["post"], echoed, "{words:?}");
        }

        let invocation = proposal(&["posts", "get", "--post-id", "0"], None);
        assert_eq!(invocation.input, json!({"postId": 0}));
        let error = invoke_with(&invocation.capability, invocation.input, |_| {
            unreachable!("an out-of-range ID cannot call HTTP")
        })
        .expect_err("the bound is enforced in invoke");
        assert_eq!(error.code(), "invalid-input");
    }

    /// The rendered text is plain: the SDK's clap is built without `color`, so no escape byte can
    /// reach a model's transcript.
    #[test]
    fn no_rendered_text_contains_an_escape_byte() {
        for words in [
            &["--help"][..],
            &["--version"][..],
            &["posts", "get", "--help"][..],
            &["posts", "create", "--help"][..],
            &["bogus"][..],
            &["posts", "get"][..],
        ] {
            let (stdout, stderr, _) = rendered(words, None);
            assert!(!stdout.contains('\u{1b}'), "{words:?}: {stdout:?}");
            assert!(!stderr.contains('\u{1b}'), "{words:?}: {stderr:?}");
        }
    }

    /// Every capability the word can propose is one the manifest declares. Without this, a renamed
    /// capability would be discovered by a model at runtime as an authorization denial.
    #[test]
    fn every_dispatch_target_is_declared_in_the_manifest() {
        let declared: Vec<String> = JsonPlaceholder::manifest()
            .capabilities
            .iter()
            .map(|capability| capability.id.to_string())
            .collect();
        let proposed: Vec<String> = [
            &["posts", "get", "--post-id", "1"][..],
            &[
                "posts",
                "create",
                "--user-id",
                "1",
                "--title",
                "t",
                "--body",
                "b",
            ][..],
        ]
        .into_iter()
        .map(|words| proposal(words, None).capability.to_string())
        .collect();
        assert_eq!(proposed, declared);
    }
}
