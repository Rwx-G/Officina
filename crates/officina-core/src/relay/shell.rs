//! The command lexer of the gate (AD-27, D-10, D-11): it reads a Bash
//! command line the way bash splits it and reports every program the line
//! can start. Command words are found through quotes, prefix assignments,
//! reserved words and wrappers; the text of substitutions, of shell
//! interpreters and of `eval` is lexed again; the one-liners of other
//! interpreters are scanned for whole words. It runs nothing, reads nothing
//! and decides nothing: [`super::gate`] matches the result against the
//! guarded list.
//!
//! Bounds (AD-27, AD-28): every text is read in one pass, nested texts are
//! lexed at most [`MAX_DEPTH`] levels deep, an input over
//! [`MAX_INPUT_BYTES`] is refused, constructs nested deeper than
//! [`MAX_NESTING`] are refused, and no regular expression is used. Every byte
//! the lexer visits is counted in [`Lexed::steps`], so linearity is tested on
//! counts rather than on time.
//!
//! Where the lexer cannot tell what bash would do, it reports more, never
//! less: a guarded repository then gets a false positive and a retry, never a
//! silent pass (D-10). Facts about the Bash the sessions run (Git for
//! Windows) that shaped a rule are recorded in story 2.3's Debug Log.

/// Longest command the lexer reads (AD-27). Longer input is an error, so the
/// gate denies it in a guarded repository.
pub const MAX_INPUT_BYTES: usize = 256 * 1024;

/// Deepest nesting of lexed texts (AD-27): a substitution, a shell
/// interpreter's `-c` text, `eval` and their kin each add one level to the
/// command line's own level 0.
pub const MAX_DEPTH: usize = 3;

/// Deepest nesting of constructs inside one text (`${`, extended globs,
/// arrays, quotes inside them). It bounds the lexer's own recursion, so a
/// crafted command cannot exhaust the stack.
pub const MAX_NESTING: usize = 64;

/// Control operators after which the next word is in command position. The
/// last three end a branch of `case`.
pub const SEPARATORS: &[&str] = &[
    "\n", ";", "&", "&&", "|", "||", "|&", "(", ")", ";;", ";&", ";;&",
];

/// Reserved words. In command position each keeps the next word in command
/// position, except `for` and `select` (a variable name follows), `case` (a
/// subject follows) and `function` (a name follows). `time` is a wrapper
/// here, because it takes options.
pub const RESERVED_WORDS: &[&str] = &[
    "!", "{", "}", "if", "then", "elif", "else", "fi", "while", "until", "do", "done", "for",
    "select", "case", "esac", "function", "coproc",
];

/// Redirection operators. Their targets are not words of the command; a
/// file descriptor number or `{name}` may precede them.
pub const REDIRECTIONS: &[&str] = &[
    "<", ">", ">>", ">|", "<>", "<&", ">&", "&>", "&>>", "<<", "<<-", "<<<",
];

/// What a wrapper does beyond skipping its options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WrapperKind {
    /// Options, then the command.
    Plain,
    /// `NAME=value` operands are assignments; [`ENV_SPLIT_OPTIONS`] carry a
    /// command line.
    Env,
    /// [`COMMAND_QUERY_OPTIONS`] only look the name up.
    Command,
    /// [`XARGS_REPLACE_OPTIONS`] set a string replaced in the command.
    Xargs,
    /// Without [`WATCH_EXEC_OPTIONS`] the operands are joined and run by a
    /// shell.
    Watch,
}

/// A program that runs another one given as its operands (AD-27, D-10).
#[derive(Debug)]
pub struct Wrapper {
    pub name: &'static str,
    /// The documented options that take a value. An option missing here is
    /// assumed to take none, so its value would be read as the command: add
    /// the option and its vector case, never a rule that skips a word.
    pub value_options: &'static [&'static str],
    /// Operands before the command (the duration of `timeout`).
    pub positionals: usize,
    pub kind: WrapperKind,
}

/// The wrappers the lexer looks through, each with its value-taking options.
pub const WRAPPERS: &[Wrapper] = &[
    Wrapper {
        name: "env",
        value_options: &["-u", "--unset", "-C", "--chdir", "-a", "--argv0"],
        positionals: 0,
        kind: WrapperKind::Env,
    },
    Wrapper {
        name: "sudo",
        value_options: &[
            "-u",
            "--user",
            "-g",
            "--group",
            "-C",
            "--close-from",
            "-D",
            "--chdir",
            "-p",
            "--prompt",
            "-r",
            "--role",
            "-t",
            "--type",
            "-T",
            "--command-timeout",
            "-U",
            "--other-user",
            "-R",
            "--chroot",
            "-a",
            "-c",
            "--login-class",
        ],
        positionals: 0,
        kind: WrapperKind::Plain,
    },
    Wrapper {
        name: "doas",
        value_options: &["-u", "-C", "-a"],
        positionals: 0,
        kind: WrapperKind::Plain,
    },
    Wrapper {
        name: "nohup",
        value_options: &[],
        positionals: 0,
        kind: WrapperKind::Plain,
    },
    Wrapper {
        name: "nice",
        value_options: &["-n", "--adjustment"],
        positionals: 0,
        kind: WrapperKind::Plain,
    },
    Wrapper {
        name: "time",
        value_options: &["-f", "--format", "-o", "--output"],
        positionals: 0,
        kind: WrapperKind::Plain,
    },
    Wrapper {
        name: "command",
        value_options: &[],
        positionals: 0,
        kind: WrapperKind::Command,
    },
    Wrapper {
        name: "exec",
        value_options: &["-a"],
        positionals: 0,
        kind: WrapperKind::Plain,
    },
    Wrapper {
        name: "builtin",
        value_options: &[],
        positionals: 0,
        kind: WrapperKind::Plain,
    },
    Wrapper {
        name: "stdbuf",
        value_options: &["-i", "--input", "-o", "--output", "-e", "--error"],
        positionals: 0,
        kind: WrapperKind::Plain,
    },
    Wrapper {
        name: "timeout",
        value_options: &["-s", "--signal", "-k", "--kill-after"],
        positionals: 1,
        kind: WrapperKind::Plain,
    },
    Wrapper {
        name: "xargs",
        value_options: &[
            "-a",
            "--arg-file",
            "-d",
            "--delimiter",
            "-E",
            "-I",
            "-L",
            "--max-lines",
            "-n",
            "--max-args",
            "-P",
            "--max-procs",
            "-s",
            "--max-chars",
            "--process-slot-var",
        ],
        positionals: 0,
        kind: WrapperKind::Xargs,
    },
    Wrapper {
        name: "watch",
        value_options: &["-n", "--interval", "-q", "--equexit"],
        positionals: 0,
        kind: WrapperKind::Watch,
    },
    Wrapper {
        name: "setsid",
        value_options: &[],
        positionals: 0,
        kind: WrapperKind::Plain,
    },
    Wrapper {
        name: "winpty",
        value_options: &[],
        positionals: 0,
        kind: WrapperKind::Plain,
    },
    Wrapper {
        name: "ssh-agent",
        value_options: &["-a", "-E", "-O", "-P", "-t"],
        positionals: 0,
        kind: WrapperKind::Plain,
    },
    Wrapper {
        name: "strace",
        value_options: &[
            "-b",
            "--buffer-size",
            "-m",
            "--mask",
            "-o",
            "--output",
            "-p",
            "--pid",
            "-S",
            "--flush-period",
        ],
        positionals: 0,
        kind: WrapperKind::Plain,
    },
];

/// `env` options whose value is itself a command line, lexed again.
pub const ENV_SPLIT_OPTIONS: &[&str] = &["-S", "--split-string"];
/// `command` options that print where a name resolves instead of running it.
pub const COMMAND_QUERY_OPTIONS: &[&str] = &["-v", "-V"];
/// `xargs` options that set the replace string; without a value it is
/// [`XARGS_DEFAULT_REPLACE`].
pub const XARGS_REPLACE_OPTIONS: &[&str] = &["-I", "-i", "--replace"];
/// The replace string of `xargs -i` and `--replace` without a value.
pub const XARGS_DEFAULT_REPLACE: &str = "{}";
/// `watch` options that run the operands without a shell.
pub const WATCH_EXEC_OPTIONS: &[&str] = &["-x", "--exec"];

/// `find` runs the words after one of [`FIND_EXEC_ACTIONS`] up to one of
/// [`FIND_EXEC_TERMINATORS`] (AD-27).
pub const FIND_PROGRAM: &str = "find";
/// See [`FIND_PROGRAM`].
pub const FIND_EXEC_ACTIONS: &[&str] = &["-exec", "-execdir", "-ok", "-okdir"];
/// See [`FIND_PROGRAM`].
pub const FIND_EXEC_TERMINATORS: &[&str] = &[";", "+"];

/// Shells whose command text is lexed again (AD-27). A short option cluster
/// holding [`SHELL_COMMAND_LETTER`] makes the first operand the text (D-10).
pub const SHELL_INTERPRETERS: &[&str] = &["bash", "sh", "zsh", "dash", "ksh"];
/// Shell options that take a value, so the text after them is found.
pub const SHELL_VALUE_OPTIONS: &[&str] = &["-o", "+o", "-O", "+O", "--rcfile", "--init-file"];
/// The option letter that makes a shell run its first operand.
pub const SHELL_COMMAND_LETTER: char = 'c';
/// The option letter that makes a shell read its program from stdin.
pub const SHELL_STDIN_LETTER: char = 's';

/// An interpreter whose one-liners are scanned for whole words, not lexed
/// (AD-27, D-10).
#[derive(Debug)]
pub struct ScriptInterpreter {
    /// A program name; a trailing `*` is a prefix (`python3.12`).
    pub name: &'static str,
    /// Options that give the program inline, so stdin is not the program.
    pub program_flags: &'static [&'static str],
    /// Options that take a value, so a script operand is found after them.
    pub value_options: &'static [&'static str],
}

/// The scanned interpreters. Every argument is scanned, not only the inline
/// program: a one-liner can run its own arguments.
pub const SCRIPT_INTERPRETERS: &[ScriptInterpreter] = &[
    ScriptInterpreter {
        name: "python*",
        program_flags: &["-c", "-m"],
        value_options: &["-W", "-X"],
    },
    ScriptInterpreter {
        name: "py",
        program_flags: &["-c", "-m"],
        value_options: &["-W", "-X"],
    },
    ScriptInterpreter {
        name: "node",
        program_flags: &["-e", "-p", "--eval", "--print"],
        value_options: &["-r", "--require"],
    },
    ScriptInterpreter {
        name: "perl*",
        program_flags: &["-e", "-E"],
        value_options: &[],
    },
    ScriptInterpreter {
        name: "ruby",
        program_flags: &["-e"],
        value_options: &["-I", "-r"],
    },
];

/// Script operands that name stdin.
pub const STDIN_PATHS: &[&str] = &["-", "/dev/stdin", "/dev/fd/0", "/proc/self/fd/0"];

/// `cmd.exe`: its arguments are scanned after caret removal (D-10).
pub const CMD_INTERPRETER: &str = "cmd";
/// `cmd` switches after which the rest of the line is the command, attached
/// or not (`/c`, `/C`, Git Bash's `//c`, `/cssh`).
pub const CMD_COMMAND_SWITCHES: &[&str] = &["c", "k", "r"];
/// The `cmd` switch that turns on `!name!` expansion.
pub const CMD_DELAYED_EXPANSION_SWITCH: &str = "v";

/// The bash array whose `[key]=path` elements bind a command name to a
/// program, like `hash -p`: the value is a program in command position.
pub const COMMAND_BINDING_PROGRAM: &str = "BASH_CMDS";
/// The bash array whose `[key]=line` elements are aliases, like the `alias`
/// builtin: the value is a command line, lexed again.
pub const COMMAND_BINDING_ALIAS: &str = "BASH_ALIASES";

/// PowerShell: every argument is scanned, and an encoded command is denied
/// outright since base64 cannot be scanned (D-10).
pub const POWERSHELL_INTERPRETERS: &[&str] = &["powershell", "pwsh"];
/// PowerShell's encoded-command parameter. Every prefix of it is accepted by
/// PowerShell, so every prefix is denied.
pub const ENCODED_COMMAND_PARAMETER: &str = "encodedcommand";
/// Other spellings PowerShell documents for the encoded command.
pub const ENCODED_COMMAND_ALIASES: &[&str] = &["ec"];

/// Every spelling of the encoded-command parameter, without its leading
/// dash: each prefix of [`ENCODED_COMMAND_PARAMETER`] and each alias.
pub fn encoded_command_spellings() -> impl Iterator<Item = &'static str> {
    (1..=ENCODED_COMMAND_PARAMETER.len())
        .map(|end| &ENCODED_COMMAND_PARAMETER[..end])
        .chain(ENCODED_COMMAND_ALIASES.iter().copied())
}

/// The spelling of an encoded-command parameter `arg` uses (`-enc`,
/// `--EncodedCommand`, `/e`, `-ec:...`), folded, if it is one.
pub fn encoded_command_parameter(arg: &str) -> Option<&'static str> {
    let bare = arg.strip_prefix("--").or_else(|| arg.strip_prefix('-'));
    let bare = bare.or_else(|| {
        let stripped = arg.trim_start_matches('/');
        (stripped.len() < arg.len()).then_some(stripped)
    })?;
    let name = fold(bare.split(':').next().unwrap_or_default());
    encoded_command_spellings().find(|spelling| *spelling == name)
}

/// A program that starts the program named in its arguments; the arguments
/// are scanned for whole words.
#[derive(Debug)]
pub struct Launcher {
    pub name: &'static str,
    /// Its arguments pass through `cmd.exe`: carets are removed and `%name%`
    /// makes the text computed.
    pub cmd_parsed: bool,
}

/// The launchers: Windows programs documented to start a command line, plus
/// Git Bash's terminal. Git Bash's `start` is a script that hands its
/// arguments to `cmd //c start`.
pub const LAUNCHERS: &[Launcher] = &[
    Launcher {
        name: "start",
        cmd_parsed: true,
    },
    Launcher {
        name: "runas",
        cmd_parsed: false,
    },
    Launcher {
        name: "schtasks",
        cmd_parsed: false,
    },
    Launcher {
        name: "forfiles",
        cmd_parsed: false,
    },
    Launcher {
        name: "conhost",
        cmd_parsed: false,
    },
    Launcher {
        name: "mintty",
        cmd_parsed: false,
    },
];

closed_enum! {
    /// Builtins whose operands hold shell text, a program path, or the name
    /// of a variable they assign.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum Builtin {
        /// Its operands, joined, are lexed again.
        Eval,
        /// The value of each `name=value` is lexed again.
        Alias,
        /// The action is lexed again.
        Trap,
        /// `-p path` binds a name to a program.
        Hash,
        /// Runs a script; a process substitution or stdin makes it computed.
        Source,
        /// See [`Builtin::Source`].
        Dot,
        /// The following assign the variables their operands name (D-12).
        Export,
        Declare,
        Typeset,
        Local,
        Readonly,
        Read,
        Mapfile,
        Readarray,
        Unset,
        Printf,
        Getopts,
        Let,
    }
}

impl Builtin {
    /// The builtin's name.
    pub fn name(self) -> &'static str {
        match self {
            Builtin::Eval => "eval",
            Builtin::Alias => "alias",
            Builtin::Trap => "trap",
            Builtin::Hash => "hash",
            Builtin::Source => "source",
            Builtin::Dot => ".",
            Builtin::Export => "export",
            Builtin::Declare => "declare",
            Builtin::Typeset => "typeset",
            Builtin::Local => "local",
            Builtin::Readonly => "readonly",
            Builtin::Read => "read",
            Builtin::Mapfile => "mapfile",
            Builtin::Readarray => "readarray",
            Builtin::Unset => "unset",
            Builtin::Printf => "printf",
            Builtin::Getopts => "getopts",
            Builtin::Let => "let",
        }
    }

    fn from_name(name: &str) -> Option<Builtin> {
        Builtin::ALL.iter().copied().find(|b| b.name() == name)
    }
}

closed_enum! {
    /// Why a command cannot be read. The gate denies each in a guarded
    /// repository (AD-26 step 4).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
    pub enum LexError {
        #[error("the command is longer than {MAX_INPUT_BYTES} bytes")]
        TooLong,
        #[error("a quote is not closed")]
        UnterminatedQuote,
        #[error("a substitution is not closed")]
        UnterminatedSubstitution,
        #[error("an expansion, array or pattern is not closed")]
        UnterminatedExpansion,
        #[error("text is nested more than {MAX_DEPTH} levels deep")]
        TooDeep,
        #[error("constructs are nested more than {MAX_NESTING} levels deep")]
        TooNested,
        #[error("a redirection has no target")]
        MissingRedirectionTarget,
        #[error("a line break inside a construct while a here-document waits for its body")]
        HeredocInConstruct,
    }
}

/// Where a program name was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// A word in command position, in any lexed text.
    Command,
    /// A whole word of a scanned interpreter's text.
    Scanned,
}

/// A program the line can start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    /// The word as bash builds it: quotes removed, escapes applied. Matching
    /// derives the name with [`program_name`], or reads the written form.
    pub written: String,
    pub origin: Origin,
}

/// A variable the line assigns, unsets or reads into (D-12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Assignment {
    Name(String),
    /// The name is computed (an expansion or a glob), so it can be any.
    Computed,
}

/// One use of a lexer constant, recorded so the vector set's coverage test
/// can prove every entry is exercised by a case (AC 9).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Seen {
    Separator(&'static str),
    Reserved(&'static str),
    Redirection(&'static str),
    Wrapper(&'static str),
    WrapperOption {
        wrapper: &'static str,
        option: &'static str,
    },
    EnvSplit(&'static str),
    CommandQuery(&'static str),
    XargsReplace(&'static str),
    WatchExec(&'static str),
    FindAction(&'static str),
    FindTerminator(&'static str),
    Shell(&'static str),
    ShellOption(&'static str),
    ShellCommandLetter,
    ShellStdinLetter,
    Script(&'static str),
    ScriptFlag {
        interpreter: &'static str,
        flag: &'static str,
    },
    StdinPath(&'static str),
    Cmd,
    CmdSwitch(&'static str),
    PowerShell(&'static str),
    EncodedSpelling(&'static str),
    Launcher(&'static str),
    Builtin(Builtin),
}

/// What [`lex`] found in a command line.
#[derive(Debug, Default)]
pub struct Lexed {
    /// Every program the line can start, in the order found.
    pub programs: Vec<Program>,
    /// The words or interpreters whose program is computed at run time (an
    /// expansion, substitution, glob, brace, ANSI-C or locale quoting in
    /// command position; a computed `-c` or `eval` text; a program read from
    /// a pipe), as written.
    pub dynamic: Vec<String>,
    /// PowerShell is given an encoded command.
    pub encoded_command: bool,
    /// Variables the line assigns, in any form.
    pub assignments: Vec<Assignment>,
    /// Every word's value, every here-document body and every scanned text,
    /// one per line, for the gate's text rules.
    pub values: String,
    /// Bytes visited, all levels together.
    pub steps: u64,
    /// The lexer constants used, for coverage.
    pub trace: Vec<Seen>,
}

/// Lexes a command line (AD-27).
///
/// ```
/// use officina_core::relay::shell::{lex, program_name};
/// let lexed = lex("cd lab && \"ss\"h host01 | tee log").unwrap();
/// let names: Vec<String> = lexed.programs.iter().map(|p| program_name(&p.written)).collect();
/// assert_eq!(names, ["cd", "ssh", "tee"]);
/// ```
pub fn lex(command: &str) -> Result<Lexed, LexError> {
    if command.len() > MAX_INPUT_BYTES {
        return Err(LexError::TooLong);
    }
    let mut out = Lexed::default();
    let mut nesting = 0;
    lex_text(&mut out, &mut nesting, command, 0, None)?;
    Ok(out)
}

/// Case folding for names: upper then lower, so the letters Windows folds to
/// ASCII (`ſ` to `S`, the dotless `ı` to `I`) compare equal to it.
pub fn fold(text: &str) -> String {
    text.to_uppercase().to_lowercase()
}

/// The last path segment of a word, folded, with what Windows ignores in a
/// file name removed (trailing dots and spaces, a `::$DATA` stream suffix),
/// but `.exe` kept: the written form D-11 matches `bash.exe` on. A name of
/// dots only (the `.` builtin) is kept as written.
pub fn written_name(word: &str) -> String {
    let segment = word.rsplit(['/', '\\']).next().unwrap_or(word);
    let mut name = fold(segment);
    loop {
        let trimmed = name.trim_end_matches(['.', ' ']);
        let trimmed = trimmed.strip_suffix("::$data").unwrap_or(trimmed);
        if trimmed.len() == name.len() || trimmed.is_empty() {
            return name;
        }
        name = trimmed.to_owned();
    }
}

/// The program name of a word (AD-27): the last segment after `/` or `\`,
/// folded, `.exe` stripped. `/usr/bin/ssh`, `C:\Windows\System32\OpenSSH\ssh.exe`
/// and `SSH.EXE` all give `ssh`.
pub fn program_name(word: &str) -> String {
    let name = written_name(word);
    match name.strip_suffix(".exe") {
        Some(stem) => stem.to_owned(),
        None => name,
    }
}

/// True when the directory part of a word has a `windows` segment
/// (`C:/Windows/System32`, `/c/windows/sysnative`), where Windows keeps the
/// WSL `bash.exe` (D-11).
pub fn under_windows_dir(word: &str) -> bool {
    let normalised = fold(word).replace('\\', "/");
    let dir = normalised.rsplit_once('/').map_or("", |(dir, _)| dir);
    dir.split('/').any(|segment| segment == "windows")
}

/// True when `name` matches a list entry; a trailing `*` makes the entry a
/// prefix. Both sides are folded.
pub fn name_matches(entry: &str, name: &str) -> bool {
    let entry = fold(entry);
    let name = fold(name);
    match entry.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => name == entry,
    }
}

fn lex_text(
    out: &mut Lexed,
    nesting: &mut usize,
    text: &str,
    depth: usize,
    latent: Option<usize>,
) -> Result<(), LexError> {
    if depth > MAX_DEPTH {
        return Err(LexError::TooDeep);
    }
    // Git Bash drops every carriage return of its input, quoted or not, so a
    // CR inside a name does not split it (Debug Log).
    out.steps += text.len() as u64;
    let bytes: Vec<u8> = text.bytes().filter(|b| *b != b'\r').collect();
    let saved = *nesting;
    let frame = Frame {
        depth,
        close: Close::Eof,
        latent,
        outer_pending: false,
    };
    let result = Level::new(&bytes, 0, frame, out, nesting)
        .parse()
        .map(|_| ());
    *nesting = saved;
    result
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Close {
    Eof,
    Paren,
}

#[derive(Debug, Clone, Copy)]
struct Frame {
    depth: usize,
    close: Close,
    /// The depth of the latent text this level belongs to, if any: text bash
    /// may evaluate later (a quoted `$(...)`), whose substitutions are
    /// reported and nothing else.
    latent: Option<usize>,
    /// The enclosing level waits for a here-document body.
    outer_pending: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CaseState {
    Subject,
    In,
    Pattern,
    Body,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Expect {
    Command,
    ForVariable,
    ForWords,
    FunctionName,
}

/// A word of a command, as bash builds it.
#[derive(Debug, Clone, Default)]
struct Word {
    /// Quotes removed, escapes applied, ANSI-C decoded; each expansion or
    /// substitution is one NUL; unquoted glob characters kept.
    text: String,
    /// The source, for messages.
    raw: String,
    /// Length of the prefix of `text` built from unquoted characters only.
    plain_len: usize,
    /// Start of the first non-literal part of `text`.
    first_non_literal: Option<usize>,
    /// End of the last non-literal part of `text`.
    literal_from: usize,
    /// A part is computed at run time: expansion, substitution, glob, brace,
    /// extended glob, locale string.
    computed: bool,
    process_substitution: bool,
    quoted: bool,
}

impl Word {
    fn non_literal(&self) -> bool {
        self.first_non_literal.is_some()
    }

    /// True when the last path segment holds no non-literal part.
    fn last_segment_literal(&self) -> bool {
        let start = self.text.rfind(['/', '\\']).map_or(0, |i| i + 1);
        start >= self.literal_from && start < self.text.len()
    }

    /// A prefix assignment (`NAME=`, `NAME+=`, `NAME[sub]=`), read on the
    /// unquoted prefix only, as bash does: the variable name and the offset
    /// in `text` where the value begins.
    fn assignment(&self) -> Option<(&str, usize)> {
        let plain = &self.text.as_bytes()[..self.plain_len];
        let name_end = name_length(plain)?;
        let mut i = name_end;
        if plain.get(i) == Some(&b'[') {
            i += plain[i..].iter().position(|b| *b == b']')? + 1;
        }
        if plain.get(i) == Some(&b'+') {
            i += 1;
        }
        (plain.get(i) == Some(&b'=')).then_some((&self.text[..name_end], i + 1))
    }

    fn assignment_name(&self) -> Option<&str> {
        self.assignment().map(|(name, _)| name)
    }

    /// True when a part computed at run time (an expansion, substitution,
    /// glob, brace or extended glob) reaches into `self.text[from..]`. Quoted
    /// and ANSI-C text is decoded at lex time, so it is not computed and a
    /// name or value written with escapes stays known.
    fn computed_from(&self, from: usize) -> bool {
        self.computed && self.literal_from > from
    }

    fn reserved(&self) -> Option<&'static str> {
        if self.quoted || self.computed && !matches!(self.text.as_str(), "{" | "}") {
            return None;
        }
        RESERVED_WORDS
            .iter()
            .copied()
            .find(|w| *w == self.text.as_str())
    }
}

/// Length of a shell name at the start of `bytes`.
fn name_length(bytes: &[u8]) -> Option<usize> {
    let first = bytes.first()?;
    if !(first.is_ascii_alphabetic() || *first == b'_') {
        return None;
    }
    Some(
        bytes
            .iter()
            .take_while(|b| b.is_ascii_alphanumeric() || **b == b'_')
            .count(),
    )
}

#[derive(Debug, Default)]
struct WordBuf {
    bytes: Vec<u8>,
    plain_len: usize,
    first_non_literal: Option<usize>,
    literal_from: usize,
    computed: bool,
    process_substitution: bool,
    quoted: bool,
    /// The last byte, when it was pushed unquoted (for `@(` and `=(`).
    last_unquoted: Option<u8>,
}

impl WordBuf {
    fn plain(&mut self, byte: u8) {
        let at_plain = self.plain_len == self.bytes.len();
        self.bytes.push(byte);
        if at_plain {
            self.plain_len += 1;
        }
        self.last_unquoted = Some(byte);
    }

    fn quoted_byte(&mut self, byte: u8) {
        self.bytes.push(byte);
        self.quoted = true;
        self.last_unquoted = None;
    }

    fn glob(&mut self, byte: u8) {
        let start = self.bytes.len();
        self.plain(byte);
        self.first_non_literal.get_or_insert(start);
        self.literal_from = self.bytes.len();
        self.computed = true;
    }

    fn non_literal(&mut self, bytes: &[u8], computed: bool) {
        let start = self.bytes.len();
        self.bytes.extend_from_slice(bytes);
        self.first_non_literal.get_or_insert(start);
        self.literal_from = self.bytes.len();
        self.computed |= computed;
        self.last_unquoted = None;
    }

    fn expansion(&mut self) {
        self.non_literal(&[0], true);
    }

    fn finish(self, raw: &[u8]) -> Word {
        let raw = String::from_utf8_lossy(raw).into_owned();
        match String::from_utf8(self.bytes) {
            Ok(text) => Word {
                text,
                raw,
                plain_len: self.plain_len,
                first_non_literal: self.first_non_literal,
                literal_from: self.literal_from,
                computed: self.computed,
                process_substitution: self.process_substitution,
                quoted: self.quoted,
            },
            // Only ANSI-C escapes produce bytes that are not UTF-8: the word
            // is then read as non-literal throughout.
            Err(error) => {
                let text = String::from_utf8_lossy(error.as_bytes()).into_owned();
                Word {
                    literal_from: text.len(),
                    text,
                    raw,
                    plain_len: 0,
                    first_non_literal: Some(0),
                    computed: self.computed,
                    process_substitution: self.process_substitution,
                    quoted: true,
                }
            }
        }
    }
}

#[derive(Debug)]
struct Redir {
    op: &'static str,
    /// The file descriptor written before the operator, if any.
    fd: Option<String>,
    target: Option<Word>,
    /// Index of the here-document body.
    body: Option<usize>,
}

impl Redir {
    fn reads_stdin(&self) -> bool {
        matches!(self.op, "<" | "<<" | "<<-" | "<<<" | "<>" | "<&")
            && self.fd.as_deref().is_none_or(|fd| fd == "0")
    }
}

#[derive(Debug)]
struct Pending {
    delimiter: Vec<u8>,
    quoted: bool,
    strip_tabs: bool,
    index: usize,
}

#[derive(Debug, Default, Clone)]
struct Body {
    text: String,
    computed: bool,
}

#[derive(Debug, Default)]
struct Command {
    words: Vec<Word>,
    redirs: Vec<Redir>,
}

struct Level<'a, 'o> {
    b: &'a [u8],
    pos: usize,
    frame: Frame,
    out: &'o mut Lexed,
    nesting: &'o mut usize,
    subshells: usize,
    cases: Vec<CaseState>,
    at_start: bool,
    expect: Expect,
    coproc: u8,
    command: Command,
    pending: Vec<Pending>,
    bodies: Vec<Body>,
    line: Vec<Command>,
}

impl<'a, 'o> Level<'a, 'o> {
    fn new(
        b: &'a [u8],
        pos: usize,
        frame: Frame,
        out: &'o mut Lexed,
        nesting: &'o mut usize,
    ) -> Self {
        Level {
            b,
            pos,
            frame,
            out,
            nesting,
            subshells: 0,
            cases: Vec::new(),
            at_start: true,
            expect: Expect::Command,
            coproc: 0,
            command: Command::default(),
            pending: Vec::new(),
            bodies: Vec::new(),
            line: Vec::new(),
        }
    }

    // ---- input ----

    fn bump(&mut self) {
        self.pos += 1;
        self.out.steps += 1;
    }

    fn peek_raw(&self) -> Option<u8> {
        self.b.get(self.pos).copied()
    }

    /// The next byte after any line continuations, which bash removes
    /// everywhere outside single quotes and comments.
    fn peek(&mut self) -> Option<u8> {
        while self.b.get(self.pos) == Some(&b'\\') && self.b.get(self.pos + 1) == Some(&b'\n') {
            self.pos += 2;
            self.out.steps += 2;
        }
        self.peek_raw()
    }

    fn skip_blanks(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t')) {
            self.bump();
        }
    }

    fn skip_comment(&mut self) {
        while let Some(c) = self.peek_raw() {
            if c == b'\n' {
                break;
            }
            self.bump();
        }
    }

    fn enter(&mut self) -> Result<(), LexError> {
        *self.nesting += 1;
        if *self.nesting > MAX_NESTING {
            return Err(LexError::TooNested);
        }
        Ok(())
    }

    fn leave(&mut self) {
        *self.nesting -= 1;
    }

    /// A line break inside a construct while a here-document waits: bash
    /// versions disagree on where the body starts, so the line is refused.
    fn newline_in_construct(&self) -> Result<(), LexError> {
        if self.pending.is_empty() && !self.frame.outer_pending {
            Ok(())
        } else {
            Err(LexError::HeredocInConstruct)
        }
    }

    // ---- recording ----

    fn in_latent_root(&self) -> bool {
        self.frame
            .latent
            .is_some_and(|root| self.frame.depth <= root)
    }

    fn trace(&mut self, seen: Seen) {
        if self.frame.latent.is_none() {
            self.out.trace.push(seen);
        }
    }

    fn program(&mut self, written: &str, origin: Origin) {
        if !self.in_latent_root() {
            self.out.programs.push(Program {
                written: written.to_owned(),
                origin,
            });
        }
    }

    fn dynamic(&mut self, raw: &str) {
        if self.frame.latent.is_none() {
            self.out.dynamic.push(raw.to_owned());
        }
    }

    fn assign(&mut self, assignment: Assignment) {
        if self.frame.latent.is_none() {
            self.out.assignments.push(assignment);
        }
    }

    fn value(&mut self, text: &str) {
        if self.frame.latent.is_none() {
            self.out.values.push_str(text);
            self.out.values.push('\n');
        }
    }

    fn lex_nested(&mut self, text: &str) -> Result<(), LexError> {
        lex_text(
            self.out,
            self.nesting,
            text,
            self.frame.depth + 1,
            self.frame.latent,
        )
    }

    /// Text bash may evaluate later (a prompt, an arithmetic subscript): its
    /// substitutions are reported; errors and everything else are ignored.
    fn latent(&mut self, word: &Word) {
        if self.frame.latent.is_none() && (word.text.contains("$(") || word.text.contains('`')) {
            let depth = self.frame.depth + 1;
            let _ = lex_text(self.out, self.nesting, &word.text, depth, Some(depth));
        }
    }

    // ---- the level ----

    /// Reads the level to its end: the end of the text, or the `)` closing
    /// a substitution. Returns the position after it.
    fn parse(mut self) -> Result<usize, LexError> {
        loop {
            self.skip_blanks();
            let Some(c) = self.peek() else {
                if self.frame.close == Close::Paren {
                    return Err(LexError::UnterminatedSubstitution);
                }
                self.end_line()?;
                return Ok(self.pos);
            };
            match c {
                b'\n' => {
                    if self.frame.outer_pending {
                        return Err(LexError::HeredocInConstruct);
                    }
                    self.bump();
                    self.trace(Seen::Separator("\n"));
                    self.end_command();
                    self.read_heredocs()?;
                    self.end_line()?;
                }
                b'#' => self.skip_comment(),
                b';' => self.semicolon(),
                b'&' => self.ampersand()?,
                b'|' => self.pipe(),
                b'(' => self.open_paren(),
                b')' => {
                    if self.close_paren() {
                        self.end_line()?;
                        return Ok(self.pos);
                    }
                }
                b'<' | b'>' if !self.procsub_ahead() => self.redirection(None)?,
                _ => {
                    let word = self.read_word(WordBuf::default(), self.pos)?;
                    if self.fd_prefix(&word) {
                        self.redirection(Some(word.text))?;
                    } else {
                        self.on_word(word);
                    }
                }
            }
        }
    }

    fn procsub_ahead(&self) -> bool {
        matches!(self.peek_raw(), Some(b'<' | b'>')) && self.b.get(self.pos + 1) == Some(&b'(')
    }

    /// A word of digits or `{name}` written right before `<` or `>`.
    fn fd_prefix(&self, word: &Word) -> bool {
        if !matches!(self.peek_raw(), Some(b'<' | b'>')) || word.quoted {
            return false;
        }
        let text = word.text.as_bytes();
        let digits = !text.is_empty() && text.iter().all(u8::is_ascii_digit);
        let named = text.len() > 2
            && text[0] == b'{'
            && text[text.len() - 1] == b'}'
            && name_length(&text[1..text.len() - 1]) == Some(text.len() - 2);
        digits || named
    }

    fn separator(&mut self, op: &'static str) {
        self.trace(Seen::Separator(op));
        self.end_command();
        self.at_start = true;
        self.expect = Expect::Command;
    }

    fn semicolon(&mut self) {
        self.bump();
        let op = if self.peek() == Some(b';') {
            self.bump();
            if self.peek() == Some(b'&') {
                self.bump();
                ";;&"
            } else {
                ";;"
            }
        } else if self.peek() == Some(b'&') {
            self.bump();
            ";&"
        } else {
            ";"
        };
        self.separator(op);
        if op != ";"
            && let Some(state) = self.cases.last_mut()
            && *state == CaseState::Body
        {
            *state = CaseState::Pattern;
        }
    }

    fn ampersand(&mut self) -> Result<(), LexError> {
        self.bump();
        match self.peek() {
            Some(b'&') => {
                self.bump();
                self.separator("&&");
            }
            Some(b'>') => {
                self.bump();
                let op = if self.peek() == Some(b'>') {
                    self.bump();
                    "&>>"
                } else {
                    "&>"
                };
                self.redirect_to(op, None)?;
            }
            _ => self.separator("&"),
        }
        Ok(())
    }

    fn pipe(&mut self) {
        self.bump();
        if self.cases.last() == Some(&CaseState::Pattern) {
            return;
        }
        let op = match self.peek() {
            Some(b'|') => {
                self.bump();
                "||"
            }
            Some(b'&') => {
                self.bump();
                "|&"
            }
            _ => "|",
        };
        self.separator(op);
    }

    fn open_paren(&mut self) {
        if self.cases.last() == Some(&CaseState::Pattern) {
            self.bump();
            return;
        }
        let arithmetic_command = self.at_start && self.expect == Expect::Command;
        let for_arithmetic = self.expect == Expect::ForVariable;
        if (arithmetic_command || for_arithmetic)
            && self.b.get(self.pos + 1) == Some(&b'(')
            && let Some(end) = self.arithmetic_end(self.pos + 2, for_arithmetic)
        {
            self.out.steps += (end - self.pos) as u64;
            self.pos = end;
            if for_arithmetic {
                self.expect = Expect::ForWords;
            }
            return;
        }
        self.bump();
        self.separator("(");
        self.subshells += 1;
    }

    /// Returns true when the `)` closes this level.
    fn close_paren(&mut self) -> bool {
        if let Some(state) = self.cases.last_mut()
            && *state == CaseState::Pattern
        {
            *state = CaseState::Body;
            self.bump();
            self.trace(Seen::Separator(")"));
            self.at_start = true;
            self.expect = Expect::Command;
            return false;
        }
        self.bump();
        if self.subshells == 0 && self.frame.close == Close::Paren {
            self.end_command();
            return true;
        }
        self.subshells = self.subshells.saturating_sub(1);
        self.separator(")");
        false
    }

    /// The end of `((...))` or `$((...))` starting at `from`, just inside
    /// both parentheses, when the text is plain arithmetic: names, numbers,
    /// operators and simple parameter expansions. Anything else (a
    /// substitution, a quote, a line break) and a closing `) )` make bash
    /// read it as nested parentheses, and the lexer does the same.
    fn arithmetic_end(&mut self, from: usize, allow_semicolon: bool) -> Option<usize> {
        let b = self.b;
        let mut depth = 2usize;
        let mut i = from;
        while let Some(&c) = b.get(i) {
            self.out.steps += 1;
            match c {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 1 {
                        return (b.get(i + 1) == Some(&b')')).then_some(i + 2);
                    }
                }
                b'$' => match b.get(i + 1) {
                    Some(n) if n.is_ascii_alphanumeric() || b"_@*#?-$!".contains(n) => i += 1,
                    Some(b'{') => {
                        i += 2;
                        loop {
                            match b.get(i) {
                                Some(b'}') => break,
                                Some(n) if n.is_ascii_alphanumeric() || b"_#@*[]:-".contains(n) => {
                                    i += 1;
                                }
                                _ => return None,
                            }
                        }
                    }
                    _ => return None,
                },
                b';' if allow_semicolon => {}
                c if c.is_ascii_alphanumeric() || b" \t_+-*/%<>=!&|^~?:,.#[]\"".contains(&c) => {}
                _ => return None,
            }
            i += 1;
        }
        None
    }

    fn on_word(&mut self, word: Word) {
        match self.cases.last().copied() {
            Some(CaseState::Subject) => {
                self.set_case(CaseState::In);
                return;
            }
            Some(CaseState::In) => {
                self.set_case(CaseState::Pattern);
                return;
            }
            Some(CaseState::Pattern) => {
                if word.reserved() == Some("esac") {
                    self.trace(Seen::Reserved("esac"));
                    self.cases.pop();
                    self.at_start = true;
                }
                return;
            }
            _ => {}
        }
        match self.expect {
            Expect::ForVariable => {
                self.declared_name(&word);
                self.expect = Expect::ForWords;
                return;
            }
            Expect::ForWords => return,
            Expect::FunctionName => {
                self.expect = Expect::Command;
                self.at_start = true;
                return;
            }
            Expect::Command => {}
        }
        if self.at_start
            && let Some(reserved) = word.reserved()
        {
            self.trace(Seen::Reserved(reserved));
            match reserved {
                "case" => self.cases.push(CaseState::Subject),
                "esac" => {
                    self.cases.pop();
                }
                "for" | "select" => self.expect = Expect::ForVariable,
                "function" => self.expect = Expect::FunctionName,
                "coproc" => self.coproc = 2,
                _ => {}
            }
            return;
        }
        // `coproc NAME { ...; }`: the word after the first may start the
        // compound command, so it is read in command position too.
        if self.coproc == 1 {
            self.coproc = 0;
            self.end_command();
            self.at_start = true;
            self.on_word(word);
            return;
        }
        if self.coproc == 2 {
            self.coproc = 1;
        }
        self.command.words.push(word);
        self.at_start = false;
    }

    fn set_case(&mut self, state: CaseState) {
        if let Some(top) = self.cases.last_mut() {
            *top = state;
        }
    }

    fn end_command(&mut self) {
        self.coproc = 0;
        if !self.command.words.is_empty() || !self.command.redirs.is_empty() {
            let command = std::mem::take(&mut self.command);
            self.line.push(command);
        }
    }

    /// Analyses the commands of the line once its here-document bodies are
    /// read.
    fn end_line(&mut self) -> Result<(), LexError> {
        self.end_command();
        self.pending.clear();
        let line = std::mem::take(&mut self.line);
        if self.in_latent_root() {
            return Ok(());
        }
        for command in &line {
            self.analyse(command)?;
        }
        Ok(())
    }

    // ---- redirections and here-documents ----

    fn redirection(&mut self, fd: Option<String>) -> Result<(), LexError> {
        let first = self.peek_raw().unwrap_or(b'<');
        self.bump();
        let op = if first == b'<' {
            match self.peek() {
                Some(b'<') => {
                    self.bump();
                    match self.peek() {
                        Some(b'<') => {
                            self.bump();
                            "<<<"
                        }
                        Some(b'-') => {
                            self.bump();
                            "<<-"
                        }
                        _ => "<<",
                    }
                }
                Some(b'&') => {
                    self.bump();
                    "<&"
                }
                Some(b'>') => {
                    self.bump();
                    "<>"
                }
                _ => "<",
            }
        } else {
            match self.peek() {
                Some(b'>') => {
                    self.bump();
                    ">>"
                }
                Some(b'&') => {
                    self.bump();
                    ">&"
                }
                Some(b'|') => {
                    self.bump();
                    ">|"
                }
                _ => ">",
            }
        };
        self.redirect_to(op, fd)
    }

    fn redirect_to(&mut self, op: &'static str, fd: Option<String>) -> Result<(), LexError> {
        self.trace(Seen::Redirection(op));
        self.skip_blanks();
        if matches!(op, "<<" | "<<-") {
            let (delimiter, quoted) = self.heredoc_delimiter()?;
            let index = self.bodies.len();
            self.bodies.push(Body::default());
            self.pending.push(Pending {
                delimiter,
                quoted,
                strip_tabs: op == "<<-",
                index,
            });
            self.command.redirs.push(Redir {
                op,
                fd,
                target: None,
                body: Some(index),
            });
            return Ok(());
        }
        let start = self.pos;
        let target = self.read_word(WordBuf::default(), start)?;
        if self.pos == start {
            return Err(LexError::MissingRedirectionTarget);
        }
        self.latent(&target);
        self.command.redirs.push(Redir {
            op,
            fd,
            target: Some(target),
            body: None,
        });
        Ok(())
    }

    /// The delimiter of a here-document: quote removal only, no expansion.
    /// Any quote or backslash makes the body literal.
    fn heredoc_delimiter(&mut self) -> Result<(Vec<u8>, bool), LexError> {
        let mut delimiter = Vec::new();
        let mut quoted = false;
        while let Some(c) = self.peek() {
            match c {
                b' ' | b'\t' | b'\n' | b';' | b'&' | b'|' | b'(' | b')' | b'<' | b'>' => break,
                b'\\' => {
                    self.bump();
                    quoted = true;
                    if let Some(next) = self.peek_raw() {
                        self.bump();
                        delimiter.push(next);
                    }
                }
                b'\'' => {
                    self.bump();
                    quoted = true;
                    loop {
                        match self.peek_raw() {
                            None => return Err(LexError::UnterminatedQuote),
                            Some(b'\'') => {
                                self.bump();
                                break;
                            }
                            Some(next) => {
                                if next == b'\n' {
                                    self.newline_in_construct()?;
                                }
                                self.bump();
                                delimiter.push(next);
                            }
                        }
                    }
                }
                b'"' => {
                    self.bump();
                    quoted = true;
                    loop {
                        match self.peek() {
                            None => return Err(LexError::UnterminatedQuote),
                            Some(b'"') => {
                                self.bump();
                                break;
                            }
                            Some(b'\\') => {
                                self.bump();
                                match self.peek_raw() {
                                    Some(next @ (b'$' | b'`' | b'"' | b'\\')) => {
                                        self.bump();
                                        delimiter.push(next);
                                    }
                                    _ => delimiter.push(b'\\'),
                                }
                            }
                            Some(next) => {
                                if next == b'\n' {
                                    self.newline_in_construct()?;
                                }
                                self.bump();
                                delimiter.push(next);
                            }
                        }
                    }
                }
                _ => {
                    self.bump();
                    delimiter.push(c);
                }
            }
        }
        if delimiter.is_empty() && !quoted {
            return Err(LexError::MissingRedirectionTarget);
        }
        Ok((delimiter, quoted))
    }

    /// Reads the bodies of the here-documents of the line just ended, in
    /// order. With an unquoted delimiter, line continuations are joined
    /// before the delimiter is compared, as bash does (Debug Log), and the
    /// body is expanded like a double-quoted string.
    fn read_heredocs(&mut self) -> Result<(), LexError> {
        for pending in std::mem::take(&mut self.pending) {
            let mut body = Vec::new();
            while self.pos < self.b.len() {
                let line = self.heredoc_line(pending.quoted);
                let line = if pending.strip_tabs {
                    let tabs = line.iter().take_while(|b| **b == b'\t').count();
                    line[tabs..].to_vec()
                } else {
                    line
                };
                if line == pending.delimiter {
                    break;
                }
                body.extend_from_slice(&line);
                body.push(b'\n');
            }
            let text = String::from_utf8_lossy(&body).into_owned();
            let result = if pending.quoted {
                Body {
                    text,
                    computed: false,
                }
            } else {
                self.expand_body(&text)?
            };
            self.value(&result.text);
            self.bodies[pending.index] = result;
        }
        Ok(())
    }

    fn heredoc_line(&mut self, quoted: bool) -> Vec<u8> {
        let mut line = Vec::new();
        while let Some(c) = self.peek_raw() {
            self.bump();
            match c {
                b'\n' => break,
                b'\\' if !quoted => match self.peek_raw() {
                    Some(b'\n') => self.bump(),
                    Some(next) => {
                        self.bump();
                        line.extend_from_slice(&[b'\\', next]);
                    }
                    None => line.push(b'\\'),
                },
                _ => line.push(c),
            }
        }
        line
    }

    fn expand_body(&mut self, text: &str) -> Result<Body, LexError> {
        let bytes = text.as_bytes();
        let frame = Frame {
            close: Close::Eof,
            outer_pending: false,
            ..self.frame
        };
        let mut level = Level::new(bytes, 0, frame, self.out, self.nesting);
        let mut word = WordBuf::default();
        while let Some(c) = level.peek_raw() {
            match c {
                b'\\' => {
                    level.bump();
                    match level.peek_raw() {
                        Some(next @ (b'$' | b'`' | b'\\')) => {
                            level.bump();
                            word.quoted_byte(next);
                        }
                        _ => word.quoted_byte(b'\\'),
                    }
                }
                b'$' => level.dollar(&mut word, true)?,
                b'`' => level.backtick(&mut word, false)?,
                _ => {
                    level.bump();
                    word.quoted_byte(c);
                }
            }
        }
        let word = word.finish(bytes);
        Ok(Body {
            text: word.text,
            computed: word.computed,
        })
    }

    // ---- words ----

    fn read_word(&mut self, mut w: WordBuf, start: usize) -> Result<Word, LexError> {
        if self.procsub_ahead() {
            self.process_substitution(&mut w)?;
        }
        while let Some(c) = self.peek() {
            match c {
                b' ' | b'\t' | b'\n' | b';' | b'&' | b'|' | b')' | b'<' | b'>' => break,
                b'(' => {
                    if self.array_prefix(&w) {
                        self.array(&mut w)?;
                    } else if matches!(w.last_unquoted, Some(b'@' | b'!' | b'+' | b'*' | b'?')) {
                        self.extglob(&mut w)?;
                    } else {
                        break;
                    }
                }
                b'\\' => {
                    self.bump();
                    match self.peek_raw() {
                        Some(next) => {
                            self.bump();
                            w.quoted_byte(next);
                        }
                        None => w.plain(b'\\'),
                    }
                }
                b'\'' => self.single_quote(&mut w)?,
                b'"' => {
                    self.bump();
                    self.double_quote(&mut w)?;
                }
                b'$' => self.dollar(&mut w, false)?,
                b'`' => self.backtick(&mut w, false)?,
                b'*' | b'?' | b'[' | b'{' | b'}' => {
                    self.bump();
                    w.glob(c);
                }
                0 => {
                    self.bump();
                    w.expansion();
                }
                _ => {
                    self.bump();
                    w.plain(c);
                }
            }
        }
        let word = w.finish(&self.b[start..self.pos]);
        if !word.text.is_empty() || word.quoted {
            self.value(&word.text);
        }
        Ok(word)
    }

    fn array_prefix(&self, w: &WordBuf) -> bool {
        if w.plain_len != w.bytes.len() || w.bytes.last() != Some(&b'=') {
            return false;
        }
        let name_end = name_length(&w.bytes).unwrap_or(0);
        name_end > 0 && matches!(&w.bytes[name_end..], b"=" | b"+=")
    }

    fn single_quote(&mut self, w: &mut WordBuf) -> Result<(), LexError> {
        self.bump();
        w.quoted = true;
        w.last_unquoted = None;
        loop {
            let Some(c) = self.peek_raw() else {
                return Err(LexError::UnterminatedQuote);
            };
            self.bump();
            match c {
                b'\'' => return Ok(()),
                b'\n' => {
                    self.newline_in_construct()?;
                    w.quoted_byte(c);
                }
                _ => w.quoted_byte(c),
            }
        }
    }

    /// The content of a double-quoted string, after its opening quote.
    fn double_quote(&mut self, w: &mut WordBuf) -> Result<(), LexError> {
        w.quoted = true;
        w.last_unquoted = None;
        loop {
            let Some(c) = self.peek() else {
                return Err(LexError::UnterminatedQuote);
            };
            match c {
                b'"' => {
                    self.bump();
                    return Ok(());
                }
                b'\\' => {
                    self.bump();
                    match self.peek_raw() {
                        Some(next @ (b'$' | b'`' | b'"' | b'\\')) => {
                            self.bump();
                            w.quoted_byte(next);
                        }
                        _ => w.quoted_byte(b'\\'),
                    }
                }
                b'$' => self.dollar(w, true)?,
                b'`' => self.backtick(w, true)?,
                0 => {
                    self.bump();
                    w.expansion();
                }
                _ => {
                    if c == b'\n' {
                        self.newline_in_construct()?;
                    }
                    self.bump();
                    w.quoted_byte(c);
                }
            }
        }
    }

    fn dollar(&mut self, w: &mut WordBuf, in_dq: bool) -> Result<(), LexError> {
        self.bump();
        match self.peek() {
            Some(b'\'') if !in_dq => self.ansi_c(w),
            Some(b'"') if !in_dq => {
                self.bump();
                let mut inner = WordBuf::default();
                self.double_quote(&mut inner)?;
                w.quoted = true;
                w.non_literal(&inner.bytes, true);
                Ok(())
            }
            Some(b'(') => {
                self.bump();
                if self.peek_raw() == Some(b'(')
                    && let Some(end) = self.arithmetic_end(self.pos + 1, false)
                {
                    self.out.steps += (end - self.pos) as u64;
                    self.pos = end;
                    w.expansion();
                    return Ok(());
                }
                self.substitution(w)
            }
            Some(b'{') => {
                self.bump();
                self.braced(b'{', b'}', in_dq)?;
                w.expansion();
                Ok(())
            }
            Some(b'[') => {
                self.bump();
                self.braced(b'[', b']', in_dq)?;
                w.expansion();
                Ok(())
            }
            Some(c) if c == b'_' || c.is_ascii_alphabetic() => {
                while self
                    .peek()
                    .is_some_and(|c| c == b'_' || c.is_ascii_alphanumeric())
                {
                    self.bump();
                }
                w.expansion();
                Ok(())
            }
            Some(c) if c.is_ascii_digit() || b"@*#?-$!".contains(&c) => {
                self.bump();
                w.expansion();
                Ok(())
            }
            _ => {
                if in_dq {
                    w.quoted_byte(b'$');
                } else {
                    w.plain(b'$');
                }
                Ok(())
            }
        }
    }

    /// `$'...'`: decoded, so an assignment or a variable name hidden behind
    /// escapes is seen; non-literal in command position (D-10).
    fn ansi_c(&mut self, w: &mut WordBuf) -> Result<(), LexError> {
        self.bump();
        let mut value = Vec::new();
        loop {
            let Some(c) = self.peek_raw() else {
                return Err(LexError::UnterminatedQuote);
            };
            self.bump();
            match c {
                b'\'' => break,
                b'\\' => self.ansi_escape(&mut value)?,
                _ => {
                    if c == b'\n' {
                        self.newline_in_construct()?;
                    }
                    value.push(c);
                }
            }
        }
        w.quoted = true;
        w.non_literal(&value, false);
        Ok(())
    }

    fn ansi_escape(&mut self, value: &mut Vec<u8>) -> Result<(), LexError> {
        let Some(e) = self.peek_raw() else {
            return Err(LexError::UnterminatedQuote);
        };
        self.bump();
        let simple = match e {
            b'a' => Some(7),
            b'b' => Some(8),
            b'e' | b'E' => Some(0x1b),
            b'f' => Some(0x0c),
            b'n' => Some(b'\n'),
            b'r' => Some(b'\r'),
            b't' => Some(b'\t'),
            b'v' => Some(0x0b),
            b'\\' | b'\'' | b'"' | b'?' => Some(e),
            _ => None,
        };
        if let Some(byte) = simple {
            value.push(byte);
            return Ok(());
        }
        match e {
            b'0'..=b'7' => {
                let mut code = u32::from(e - b'0');
                for _ in 0..2 {
                    match self.peek_raw() {
                        Some(d @ b'0'..=b'7') => {
                            self.bump();
                            code = code * 8 + u32::from(d - b'0');
                        }
                        _ => break,
                    }
                }
                value.push((code & 0xff) as u8);
            }
            b'x' | b'u' | b'U' => {
                let max = match e {
                    b'x' => 2,
                    b'u' => 4,
                    _ => 8,
                };
                let mut code = 0u32;
                let mut digits = 0;
                while digits < max {
                    match self.peek_raw().and_then(|d| char::from(d).to_digit(16)) {
                        Some(d) => {
                            self.bump();
                            code = code.wrapping_mul(16).wrapping_add(d);
                            digits += 1;
                        }
                        None => break,
                    }
                }
                if digits == 0 {
                    value.extend_from_slice(&[b'\\', e]);
                } else if e == b'x' {
                    value.push((code & 0xff) as u8);
                } else if let Some(ch) = char::from_u32(code) {
                    let mut buf = [0u8; 4];
                    value.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                }
            }
            b'c' => match self.peek_raw() {
                Some(x) => {
                    self.bump();
                    value.push(x & 0x1f);
                }
                None => value.extend_from_slice(b"\\c"),
            },
            _ => value.extend_from_slice(&[b'\\', e]),
        }
        Ok(())
    }

    /// `$(...)`, from just after its `(`.
    fn substitution(&mut self, w: &mut WordBuf) -> Result<(), LexError> {
        let end = self.nested_level()?;
        self.pos = end;
        w.expansion();
        Ok(())
    }

    fn nested_level(&mut self) -> Result<usize, LexError> {
        let depth = self.frame.depth + 1;
        if depth > MAX_DEPTH {
            return Err(LexError::TooDeep);
        }
        let frame = Frame {
            depth,
            close: Close::Paren,
            latent: self.frame.latent,
            outer_pending: !self.pending.is_empty() || self.frame.outer_pending,
        };
        let start = self.pos;
        Level::new(self.b, start, frame, self.out, self.nesting).parse()
    }

    fn process_substitution(&mut self, w: &mut WordBuf) -> Result<(), LexError> {
        self.bump();
        self.bump();
        self.substitution(w)?;
        w.process_substitution = true;
        Ok(())
    }

    fn backtick(&mut self, w: &mut WordBuf, in_dq: bool) -> Result<(), LexError> {
        self.bump();
        let mut content = Vec::new();
        loop {
            let Some(c) = self.peek_raw() else {
                return Err(LexError::UnterminatedSubstitution);
            };
            self.bump();
            match c {
                b'`' => break,
                b'\\' => match self.peek_raw() {
                    Some(next @ (b'`' | b'\\' | b'$')) => {
                        self.bump();
                        content.push(next);
                    }
                    Some(b'"') if in_dq => {
                        self.bump();
                        content.push(b'"');
                    }
                    _ => content.push(b'\\'),
                },
                _ => {
                    if c == b'\n' {
                        self.newline_in_construct()?;
                    }
                    content.push(c);
                }
            }
        }
        let text = String::from_utf8_lossy(&content).into_owned();
        self.lex_nested(&text)?;
        w.expansion();
        Ok(())
    }

    /// `${...}` or `$[...]`, from just after the opening bracket. The value
    /// is not needed; the substitutions inside are lexed.
    fn braced(&mut self, open: u8, close: u8, in_dq: bool) -> Result<(), LexError> {
        self.enter()?;
        let mut inner = 0usize;
        let mut scratch = WordBuf::default();
        loop {
            let Some(c) = self.peek() else {
                return Err(LexError::UnterminatedExpansion);
            };
            match c {
                c if c == close => {
                    self.bump();
                    if inner == 0 {
                        break;
                    }
                    inner -= 1;
                }
                c if c == open => {
                    self.bump();
                    inner += 1;
                }
                b'\\' => {
                    self.bump();
                    if self.peek_raw().is_some() {
                        self.bump();
                    }
                }
                b'\'' if !in_dq => self.single_quote(&mut scratch)?,
                b'"' => {
                    self.bump();
                    self.double_quote(&mut scratch)?;
                }
                b'$' => self.dollar(&mut scratch, in_dq)?,
                b'`' => self.backtick(&mut scratch, in_dq)?,
                _ => {
                    if c == b'\n' {
                        self.newline_in_construct()?;
                    }
                    self.bump();
                }
            }
        }
        self.leave();
        Ok(())
    }

    /// An extended glob group `@(...)`, `!(...)`, `+(...)`, `*(...)`,
    /// `?(...)`: one computed part of the word.
    fn extglob(&mut self, w: &mut WordBuf) -> Result<(), LexError> {
        self.enter()?;
        self.bump();
        let mut depth = 1usize;
        let mut scratch = WordBuf::default();
        while depth > 0 {
            let Some(c) = self.peek() else {
                return Err(LexError::UnterminatedExpansion);
            };
            match c {
                b'(' => {
                    self.bump();
                    depth += 1;
                }
                b')' => {
                    self.bump();
                    depth -= 1;
                }
                b'\\' => {
                    self.bump();
                    if self.peek_raw().is_some() {
                        self.bump();
                    }
                }
                b'\'' => self.single_quote(&mut scratch)?,
                b'"' => {
                    self.bump();
                    self.double_quote(&mut scratch)?;
                }
                b'$' => self.dollar(&mut scratch, false)?,
                b'`' => self.backtick(&mut scratch, false)?,
                _ => {
                    if c == b'\n' {
                        self.newline_in_construct()?;
                    }
                    self.bump();
                }
            }
        }
        self.leave();
        w.expansion();
        Ok(())
    }

    /// `NAME=(...)`: the elements are words, never commands.
    fn array(&mut self, w: &mut WordBuf) -> Result<(), LexError> {
        self.enter()?;
        self.bump();
        loop {
            self.skip_blanks();
            match self.peek() {
                None => return Err(LexError::UnterminatedExpansion),
                Some(b')') => {
                    self.bump();
                    break;
                }
                Some(b'\n') => {
                    self.newline_in_construct()?;
                    self.bump();
                }
                Some(b'#') => self.skip_comment(),
                Some(b';' | b'&' | b'|' | b'<' | b'>') if !self.procsub_ahead() => self.bump(),
                Some(_) => {
                    let start = self.pos;
                    let element = self.read_word(WordBuf::default(), start)?;
                    if self.pos == start {
                        self.bump();
                    }
                    self.latent(&element);
                }
            }
        }
        self.leave();
        w.non_literal(&[0], false);
        Ok(())
    }

    // ---- analysis ----

    fn analyse(&mut self, command: &Command) -> Result<(), LexError> {
        let mut work: Vec<&[Word]> = vec![&command.words];
        while let Some(words) = work.pop() {
            self.analyse_words(words, &command.redirs, &mut work)?;
        }
        Ok(())
    }

    fn analyse_words<'w>(
        &mut self,
        words: &'w [Word],
        redirs: &[Redir],
        work: &mut Vec<&'w [Word]>,
    ) -> Result<(), LexError> {
        let mut i = 0;
        while let Some(word) = words.get(i) {
            if word.assignment_name().is_none() {
                break;
            }
            self.note_assignment(word)?;
            i += 1;
        }
        loop {
            let Some(word) = words.get(i) else {
                return Ok(());
            };
            if self.command_word(word) {
                return Ok(());
            }
            let name = program_name(&word.text);
            if let Some(wrapper) = WRAPPERS.iter().find(|w| w.name == name) {
                i = self.wrapper(wrapper, words, i + 1)?;
                continue;
            }
            let args = &words[i + 1..];
            if name == FIND_PROGRAM {
                self.find(args, work);
            } else if let Some(builtin) = Builtin::from_name(&name) {
                self.builtin(builtin, args, redirs)?;
            } else if let Some(shell) = SHELL_INTERPRETERS.iter().copied().find(|s| *s == name) {
                self.trace(Seen::Shell(shell));
                self.shell(word, args, redirs)?;
            } else if let Some(script) = SCRIPT_INTERPRETERS
                .iter()
                .find(|s| name_matches(s.name, &name))
            {
                self.trace(Seen::Script(script.name));
                self.script(script, word, args, redirs)?;
            } else if name == CMD_INTERPRETER {
                self.trace(Seen::Cmd);
                self.cmd(word, args, redirs)?;
            } else if let Some(powershell) =
                POWERSHELL_INTERPRETERS.iter().copied().find(|p| *p == name)
            {
                self.trace(Seen::PowerShell(powershell));
                self.powershell(word, args, redirs)?;
            } else if let Some(launcher) = LAUNCHERS.iter().find(|l| l.name == name) {
                self.trace(Seen::Launcher(launcher.name));
                self.launcher(launcher, word, args);
            } else {
                for arg in args {
                    self.latent(arg);
                }
            }
            return Ok(());
        }
    }

    /// Records a command word; returns true when it is dynamic (D-10). A
    /// dynamic word whose last path segment is literal is still matched by
    /// name (`$DIR/ssh`).
    fn command_word(&mut self, word: &Word) -> bool {
        let dynamic = word.non_literal() && !matches!(word.text.as_str(), "[" | "[[");
        if !dynamic || word.last_segment_literal() {
            self.program(&word.text, Origin::Command);
        }
        if dynamic {
            self.dynamic(&word.raw);
        }
        dynamic
    }

    fn wrapper(
        &mut self,
        wrapper: &'static Wrapper,
        words: &[Word],
        mut i: usize,
    ) -> Result<usize, LexError> {
        self.trace(Seen::Wrapper(wrapper.name));
        let mut positionals = wrapper.positionals;
        let mut replace: Option<String> = None;
        let mut watch_exec = false;
        let mut options_done = false;
        while let Some(arg) = words.get(i) {
            // Quotes do not matter here: bash removed them before the
            // wrapper reads its arguments.
            let v = arg.text.as_str();
            let option = !options_done && v.starts_with('-');
            if option && v == "--" {
                options_done = true;
                i += 1;
                continue;
            }
            if wrapper.kind == WrapperKind::Env && !option && v.contains('=') {
                self.env_assignment(arg)?;
                i += 1;
                continue;
            }
            if option {
                let next = words.get(i + 1);
                let consumed = if let Some(long) = v.strip_prefix("--") {
                    let (name, attached) = match long.split_once('=') {
                        Some((name, value)) => (name, Some(value)),
                        None => (long, None),
                    };
                    let option = &v[..name.len() + 2];
                    self.wrapper_option(
                        wrapper,
                        option,
                        attached,
                        next,
                        &mut replace,
                        &mut watch_exec,
                    )?
                } else {
                    self.wrapper_cluster(wrapper, v, next, &mut replace, &mut watch_exec)?
                };
                match consumed {
                    OptionOutcome::Stop => return Ok(words.len()),
                    OptionOutcome::Consumed(n) => i += 1 + n,
                }
                continue;
            }
            if positionals > 0 {
                positionals -= 1;
                i += 1;
                continue;
            }
            break;
        }
        if wrapper.kind == WrapperKind::Watch && !watch_exec && i < words.len() {
            let operands = &words[i..];
            if operands.iter().any(|w| w.computed) {
                self.dynamic(&operands[0].raw);
            }
            let text = join_values(operands);
            self.lex_nested(&text)?;
            return Ok(words.len());
        }
        // A non-empty replace string is filled from stdin at run time, so any
        // word carrying it (the command itself, or a shell's `-c` text) builds
        // a command the lexer cannot see.
        if let Some(replace) = replace
            && !replace.is_empty()
            && let Some(templated) = words[i..].iter().find(|w| w.text.contains(&replace))
        {
            self.dynamic(&templated.raw);
        }
        Ok(i)
    }

    fn wrapper_cluster(
        &mut self,
        wrapper: &'static Wrapper,
        v: &str,
        next: Option<&Word>,
        replace: &mut Option<String>,
        watch_exec: &mut bool,
    ) -> Result<OptionOutcome, LexError> {
        for (k, ch) in v.char_indices().skip(1) {
            let spelled = format!("-{ch}");
            let rest = &v[k + ch.len_utf8()..];
            let attached = (!rest.is_empty()).then_some(rest);
            let outcome =
                self.wrapper_option(wrapper, &spelled, attached, next, replace, watch_exec)?;
            match outcome {
                OptionOutcome::Consumed(0)
                    if attached.is_some() && self.option_took_value(wrapper, &spelled) =>
                {
                    return Ok(OptionOutcome::Consumed(0));
                }
                OptionOutcome::Consumed(0) => {}
                other => return Ok(other),
            }
        }
        Ok(OptionOutcome::Consumed(0))
    }

    fn option_took_value(&self, wrapper: &Wrapper, option: &str) -> bool {
        wrapper.value_options.contains(&option)
            || (wrapper.kind == WrapperKind::Env && ENV_SPLIT_OPTIONS.contains(&option))
            || (wrapper.kind == WrapperKind::Xargs && XARGS_REPLACE_OPTIONS.contains(&option))
    }

    /// One option of a wrapper, its value attached or in `next`.
    fn wrapper_option(
        &mut self,
        wrapper: &'static Wrapper,
        option: &str,
        attached: Option<&str>,
        next: Option<&Word>,
        replace: &mut Option<String>,
        watch_exec: &mut bool,
    ) -> Result<OptionOutcome, LexError> {
        let from_next = |attached: Option<&str>| -> (Option<String>, usize) {
            match attached {
                Some(value) => (Some(value.to_owned()), 0),
                None => (next.map(|w| w.text.clone()), usize::from(next.is_some())),
            }
        };
        match wrapper.kind {
            WrapperKind::Env => {
                if let Some(split) = ENV_SPLIT_OPTIONS.iter().copied().find(|o| *o == option) {
                    self.trace(Seen::EnvSplit(split));
                    let (value, used) = from_next(attached);
                    let computed = attached.is_none() && next.is_some_and(|w| w.computed);
                    if computed {
                        self.dynamic(next.map_or("", |w| w.raw.as_str()));
                    }
                    if let Some(value) = value {
                        self.lex_nested(&value)?;
                    }
                    return Ok(OptionOutcome::Consumed(used));
                }
            }
            WrapperKind::Command => {
                if let Some(query) = COMMAND_QUERY_OPTIONS.iter().copied().find(|o| *o == option) {
                    self.trace(Seen::CommandQuery(query));
                    return Ok(OptionOutcome::Stop);
                }
            }
            WrapperKind::Xargs => {
                if let Some(spelling) = XARGS_REPLACE_OPTIONS.iter().copied().find(|o| *o == option)
                {
                    self.trace(Seen::XargsReplace(spelling));
                    if wrapper.value_options.contains(&option) {
                        let (value, used) = from_next(attached);
                        self.trace(Seen::WrapperOption {
                            wrapper: wrapper.name,
                            option: spelling,
                        });
                        *replace = value;
                        return Ok(OptionOutcome::Consumed(used));
                    }
                    *replace = Some(attached.unwrap_or(XARGS_DEFAULT_REPLACE).to_owned());
                    return Ok(OptionOutcome::Consumed(0));
                }
            }
            WrapperKind::Watch => {
                if let Some(exec) = WATCH_EXEC_OPTIONS.iter().copied().find(|o| *o == option) {
                    self.trace(Seen::WatchExec(exec));
                    *watch_exec = true;
                }
            }
            WrapperKind::Plain => {}
        }
        if let Some(entry) = wrapper.value_options.iter().copied().find(|o| *o == option) {
            self.trace(Seen::WrapperOption {
                wrapper: wrapper.name,
                option: entry,
            });
            let (value, used) = from_next(attached);
            if wrapper.kind == WrapperKind::Env
                && let Some(value) = value
                && matches!(entry, "-u" | "--unset")
            {
                self.assign(Assignment::Name(value));
            }
            return Ok(OptionOutcome::Consumed(used));
        }
        Ok(OptionOutcome::Consumed(0))
    }

    fn env_assignment(&mut self, word: &Word) -> Result<(), LexError> {
        self.note_assignment(word)
    }

    /// The variable a builtin or `for` names, with no value to bind (`for
    /// HOME in`, `unset HOME`, `printf -v HOME`): its name, for D-12, and
    /// latent substitutions.
    fn declared_name(&mut self, word: &Word) {
        let name_end = word.text.find(['=', '[']).unwrap_or(word.text.len());
        let name_end = word.text[..name_end]
            .strip_suffix('+')
            .map_or(name_end, str::len);
        if word.computed && word.first_non_literal.is_some_and(|p| p < name_end) {
            self.assign(Assignment::Computed);
        } else if name_end > 0 {
            self.assign(Assignment::Name(word.text[..name_end].to_owned()));
        }
        self.latent(word);
    }

    /// Records an assignment (prefix, `env NAME=`, `declare`, D-12): the
    /// variable name, any command its value binds (`BASH_CMDS`,
    /// `BASH_ALIASES`), and latent substitutions. ANSI-C quoting is decoded,
    /// so a name or value written with escapes is still read.
    fn note_assignment(&mut self, word: &Word) -> Result<(), LexError> {
        self.declared_name(word);
        if let Some((base, value_start)) = word.assignment() {
            let value = word.text[value_start..].to_owned();
            let computed = word.computed_from(value_start);
            if base == COMMAND_BINDING_PROGRAM {
                if computed {
                    self.dynamic(&word.raw);
                } else if !value.is_empty() {
                    self.program(&value, Origin::Command);
                }
            } else if base == COMMAND_BINDING_ALIAS {
                if computed {
                    self.dynamic(&word.raw);
                } else {
                    self.lex_nested(&value)?;
                }
            }
        }
        Ok(())
    }

    fn find<'w>(&mut self, args: &'w [Word], work: &mut Vec<&'w [Word]>) {
        let mut i = 0;
        while i < args.len() {
            let Some(action) = FIND_EXEC_ACTIONS
                .iter()
                .copied()
                .find(|a| *a == args[i].text)
            else {
                self.latent(&args[i]);
                i += 1;
                continue;
            };
            self.trace(Seen::FindAction(action));
            let start = i + 1;
            let end = args[start..]
                .iter()
                .position(|a| FIND_EXEC_TERMINATORS.contains(&a.text.as_str()))
                .map_or(args.len(), |p| start + p);
            if let Some(terminator) = args
                .get(end)
                .and_then(|a| FIND_EXEC_TERMINATORS.iter().copied().find(|t| *t == a.text))
            {
                self.trace(Seen::FindTerminator(terminator));
            }
            work.push(&args[start..end]);
            i = end + 1;
        }
    }

    fn builtin(
        &mut self,
        builtin: Builtin,
        args: &[Word],
        redirs: &[Redir],
    ) -> Result<(), LexError> {
        self.trace(Seen::Builtin(builtin));
        match builtin {
            Builtin::Eval => {
                let args = match args.first() {
                    Some(first) if first.text == "--" => &args[1..],
                    _ => args,
                };
                if let Some(computed) = args.iter().find(|a| a.computed) {
                    self.dynamic(&computed.raw);
                }
                if !args.is_empty() {
                    self.lex_nested(&join_values(args))?;
                }
            }
            Builtin::Alias => {
                for arg in args {
                    if let Some((_, value)) = arg.text.split_once('=') {
                        if arg.computed {
                            self.dynamic(&arg.raw);
                        }
                        self.lex_nested(value)?;
                    }
                }
            }
            Builtin::Trap => {
                if let Some(action) = args.iter().find(|a| !a.text.starts_with('-')) {
                    if action.computed {
                        self.dynamic(&action.raw);
                    }
                    self.lex_nested(&action.text)?;
                }
            }
            Builtin::Hash => {
                let mut iter = args.iter();
                while let Some(arg) = iter.next() {
                    if let Some(attached) = arg.text.strip_prefix("-p") {
                        let path = if attached.is_empty() {
                            iter.next().map(|w| w.text.clone())
                        } else {
                            Some(attached.to_owned())
                        };
                        if let Some(path) = path {
                            self.program(&path, Origin::Command);
                        }
                    }
                }
            }
            Builtin::Source | Builtin::Dot => {
                let operand = args.iter().find(|a| a.text != "--");
                self.script_operand(
                    operand,
                    args.first().map_or("", |a| a.raw.as_str()),
                    redirs,
                    true,
                )?;
            }
            Builtin::Export
            | Builtin::Declare
            | Builtin::Typeset
            | Builtin::Local
            | Builtin::Readonly
            | Builtin::Read
            | Builtin::Mapfile
            | Builtin::Readarray
            | Builtin::Unset => {
                for arg in args {
                    if !(arg.text.starts_with('-') || arg.text.starts_with('+')) {
                        self.note_assignment(arg)?;
                    }
                }
            }
            Builtin::Printf => {
                let mut iter = args.iter();
                while let Some(arg) = iter.next() {
                    if let Some(attached) = arg.text.strip_prefix("-v") {
                        if attached.is_empty() {
                            if let Some(name) = iter.next() {
                                self.declared_name(name);
                            }
                        } else {
                            self.assign(Assignment::Name(attached.to_owned()));
                        }
                    } else {
                        self.latent(arg);
                    }
                }
            }
            Builtin::Getopts => {
                if let Some(name) = args.iter().filter(|a| !a.text.starts_with('-')).nth(1) {
                    self.declared_name(name);
                }
            }
            Builtin::Let => {
                for arg in args {
                    if let Some(len) = name_length(arg.text.as_bytes()) {
                        self.assign(Assignment::Name(arg.text[..len].to_owned()));
                    }
                }
            }
        }
        Ok(())
    }

    /// A script operand: a process substitution is computed text, a stdin
    /// path reads the program from stdin, any other file is a limit (AD-27).
    fn script_operand(
        &mut self,
        operand: Option<&Word>,
        program_raw: &str,
        redirs: &[Redir],
        shell: bool,
    ) -> Result<(), LexError> {
        match operand {
            Some(op) if op.process_substitution => {
                self.dynamic(&op.raw);
                Ok(())
            }
            Some(op) => match STDIN_PATHS.iter().copied().find(|p| *p == op.text) {
                Some(path) => {
                    self.trace(Seen::StdinPath(path));
                    self.stdin_program(program_raw, redirs, shell)
                }
                None => Ok(()),
            },
            None => Ok(()),
        }
    }

    /// An interpreter that reads its program from stdin: a here-document or
    /// a here-string is the program (lexed for a shell, scanned otherwise);
    /// a plain file is a limit; anything else (a pipe, a loop's input, a
    /// process substitution, another descriptor) is computed text.
    fn stdin_program(
        &mut self,
        program_raw: &str,
        redirs: &[Redir],
        shell: bool,
    ) -> Result<(), LexError> {
        let mut explicit = false;
        for redir in redirs.iter().filter(|r| r.reads_stdin()) {
            explicit = true;
            match redir.op {
                "<<" | "<<-" => {
                    let body = redir
                        .body
                        .and_then(|i| self.bodies.get(i))
                        .cloned()
                        .unwrap_or_default();
                    self.program_text(program_raw, &body.text, body.computed, shell)?;
                }
                "<<<" => {
                    if let Some(target) = &redir.target {
                        self.program_text(program_raw, &target.text, target.computed, shell)?;
                    }
                }
                "<" | "<>" => {
                    if redir
                        .target
                        .as_ref()
                        .is_some_and(|t| t.process_substitution)
                    {
                        self.dynamic(program_raw);
                    }
                }
                _ => self.dynamic(program_raw),
            }
        }
        if !explicit {
            self.dynamic(program_raw);
        }
        Ok(())
    }

    fn program_text(
        &mut self,
        program_raw: &str,
        text: &str,
        computed: bool,
        shell: bool,
    ) -> Result<(), LexError> {
        if shell {
            if computed {
                self.dynamic(program_raw);
            }
            self.lex_nested(text)
        } else {
            self.scan(text);
            Ok(())
        }
    }

    fn shell(&mut self, program: &Word, args: &[Word], redirs: &[Redir]) -> Result<(), LexError> {
        let mut i = 0;
        let mut command = false;
        let mut stdin = false;
        while let Some(arg) = args.get(i) {
            let v = arg.text.as_str();
            if v == "--" || v == "-" {
                i += 1;
                break;
            }
            if v.starts_with("--") {
                let name = v.split_once('=').map_or(v, |(name, _)| name);
                if let Some(entry) = SHELL_VALUE_OPTIONS.iter().copied().find(|o| *o == name) {
                    self.trace(Seen::ShellOption(entry));
                    i += if v.contains('=') { 1 } else { 2 };
                } else {
                    i += 1;
                }
                continue;
            }
            let Some(sign) = v.chars().next().filter(|c| matches!(c, '-' | '+')) else {
                break;
            };
            if v.len() < 2 {
                break;
            }
            let mut take_next = false;
            for (k, ch) in v.char_indices().skip(1) {
                if ch == SHELL_COMMAND_LETTER {
                    command = true;
                    self.trace(Seen::ShellCommandLetter);
                } else if ch == SHELL_STDIN_LETTER {
                    stdin = true;
                    self.trace(Seen::ShellStdinLetter);
                } else {
                    let spelled = format!("{sign}{ch}");
                    if let Some(entry) = SHELL_VALUE_OPTIONS.iter().copied().find(|o| *o == spelled)
                    {
                        self.trace(Seen::ShellOption(entry));
                        take_next = v[k + ch.len_utf8()..].is_empty();
                        break;
                    }
                }
            }
            i += 1 + usize::from(take_next);
        }
        let operands = args.get(i..).unwrap_or_default();
        if command {
            match operands.first() {
                None => self.dynamic(&program.raw),
                Some(text) => {
                    if text.computed {
                        self.dynamic(&text.raw);
                    }
                    self.lex_nested(&text.text)?;
                }
            }
            return Ok(());
        }
        match operands.first() {
            Some(op) if !stdin => self.script_operand(Some(op), &program.raw, redirs, true),
            _ => self.stdin_program(&program.raw, redirs, true),
        }
    }

    fn script(
        &mut self,
        interpreter: &'static ScriptInterpreter,
        program: &Word,
        args: &[Word],
        redirs: &[Redir],
    ) -> Result<(), LexError> {
        for arg in args {
            self.scan(&arg.text);
        }
        let mut i = 0;
        while let Some(arg) = args.get(i) {
            let v = arg.text.as_str();
            if v == "--" {
                i += 1;
                break;
            }
            let inline = interpreter.program_flags.iter().copied().find(|f| {
                v == *f
                    || (f.len() == 2
                        && f.starts_with('-')
                        && v.starts_with(f)
                        && !v.starts_with("--"))
            });
            if let Some(flag) = inline {
                self.trace(Seen::ScriptFlag {
                    interpreter: interpreter.name,
                    flag,
                });
                return Ok(());
            }
            if let Some(option) = interpreter.value_options.iter().copied().find(|o| *o == v) {
                self.trace(Seen::ScriptFlag {
                    interpreter: interpreter.name,
                    flag: option,
                });
                i += 2;
                continue;
            }
            if v.starts_with('-') && v.len() > 1 {
                i += 1;
                continue;
            }
            break;
        }
        match args.get(i) {
            Some(op) => self.script_operand(Some(op), &program.raw, redirs, false),
            None => self.stdin_program(&program.raw, redirs, false),
        }
    }

    fn cmd(&mut self, program: &Word, args: &[Word], redirs: &[Redir]) -> Result<(), LexError> {
        let mut command = false;
        let mut delayed = false;
        let mut text = String::new();
        for arg in args {
            let v = arg.text.as_str();
            let switch = v.trim_start_matches('/');
            if !command && v.starts_with('/') && !switch.is_empty() {
                let lower = fold(switch);
                if let Some(entry) = CMD_COMMAND_SWITCHES
                    .iter()
                    .copied()
                    .find(|s| lower.starts_with(s))
                {
                    self.trace(Seen::CmdSwitch(entry));
                    command = true;
                    text.push_str(&switch[entry.len()..]);
                    text.push(' ');
                    continue;
                }
                if lower.starts_with(CMD_DELAYED_EXPANSION_SWITCH) && !lower.ends_with("off") {
                    self.trace(Seen::CmdSwitch(CMD_DELAYED_EXPANSION_SWITCH));
                    delayed = true;
                }
            }
            text.push_str(v);
            text.push(' ');
        }
        let text = remove_carets(&text);
        self.value(&text);
        self.scan(&text);
        if count(&text, '%') >= 2 || delayed && count(&text, '!') >= 2 {
            self.dynamic(&program.raw);
        }
        if !command {
            self.stdin_program(&program.raw, redirs, false)?;
        }
        Ok(())
    }

    fn powershell(
        &mut self,
        program: &Word,
        args: &[Word],
        redirs: &[Redir],
    ) -> Result<(), LexError> {
        for arg in args {
            if let Some(spelling) = encoded_command_parameter(&arg.text) {
                self.trace(Seen::EncodedSpelling(spelling));
                if self.frame.latent.is_none() {
                    self.out.encoded_command = true;
                }
            }
            self.scan(&arg.text);
        }
        let switch =
            |a: &Word| a.text.len() > 1 && (a.text.starts_with('-') || a.text.starts_with('/'));
        if args.iter().any(|a| a.text == "-") || args.iter().all(switch) {
            self.stdin_program(&program.raw, redirs, false)?;
        }
        Ok(())
    }

    fn launcher(&mut self, launcher: &Launcher, program: &Word, args: &[Word]) {
        let text = join_values(args);
        let text = if launcher.cmd_parsed {
            remove_carets(&text)
        } else {
            text
        };
        self.value(&text);
        self.scan(&text);
        if launcher.cmd_parsed && count(&text, '%') >= 2 {
            self.dynamic(&program.raw);
        }
    }

    /// Scans a text for whole words, each a possible program name, and for
    /// an encoded PowerShell command (AD-27, D-10).
    fn scan(&mut self, text: &str) {
        let mut token = String::new();
        for ch in text.chars() {
            self.out.steps += 1;
            if ch.is_alphanumeric() || matches!(ch, '_' | '-' | '.') {
                token.push(ch);
            } else if ch != '\r' && !token.is_empty() {
                self.program(&token, Origin::Scanned);
                token.clear();
            }
        }
        if !token.is_empty() {
            self.program(&token, Origin::Scanned);
        }
        self.scan_encoded(text);
    }

    /// An encoded-command parameter after a PowerShell program, before the
    /// next separator.
    fn scan_encoded(&mut self, text: &str) {
        let mut after_powershell = false;
        let mut arg = String::new();
        for ch in text.chars().chain(std::iter::once(' ')) {
            let separator = matches!(ch, ';' | '&' | '|' | '\n');
            if ch.is_whitespace() || separator || "\"'`,[](){}".contains(ch) {
                if !arg.is_empty() {
                    if after_powershell && let Some(spelling) = encoded_command_parameter(&arg) {
                        self.trace(Seen::EncodedSpelling(spelling));
                        if self.frame.latent.is_none() {
                            self.out.encoded_command = true;
                        }
                    }
                    if POWERSHELL_INTERPRETERS.contains(&program_name(&arg).as_str()) {
                        after_powershell = true;
                    }
                    arg.clear();
                }
                if separator {
                    after_powershell = false;
                }
            } else if ch != '\r' {
                arg.push(ch);
            }
        }
    }
}

enum OptionOutcome {
    /// The option took this many following words.
    Consumed(usize),
    /// The wrapper runs nothing.
    Stop,
}

fn join_values(words: &[Word]) -> String {
    let mut text = String::new();
    for (i, word) in words.iter().enumerate() {
        if i > 0 {
            text.push(' ');
        }
        text.push_str(&word.text);
    }
    text
}

/// `cmd.exe` caret escapes: `^x` is `x`, `^^` is `^`.
fn remove_carets(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '^' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn count(text: &str, needle: char) -> usize {
    text.chars().filter(|c| *c == needle).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(command: &str) -> Vec<String> {
        lex(command)
            .unwrap()
            .programs
            .iter()
            .filter(|p| p.origin == Origin::Command)
            .map(|p| program_name(&p.written))
            .collect()
    }

    fn words_of(command: &str) -> Vec<String> {
        lex(command)
            .unwrap()
            .programs
            .iter()
            .map(|p| p.written.clone())
            .collect()
    }

    #[test]
    fn single_quotes_are_literal() {
        assert_eq!(words_of("'s\"s$h' x"), ["s\"s$h"]);
        assert!(lex("echo '$(ssh host)'").unwrap().dynamic.is_empty());
        assert_eq!(names("echo 'a; ssh host'"), ["echo"]);
        assert_eq!(
            words_of("'ss\\\nh' x"),
            ["ss\\\nh"],
            "no continuation inside"
        );
    }

    #[test]
    fn double_quote_escapes() {
        assert_eq!(words_of(r#""a\"b\\c\$d\`e\x" y"#), [r#"a"b\c$d`e\x"#]);
        let lexed = lex("\"$X\" y").unwrap();
        assert_eq!(lexed.dynamic, ["\"$X\""]);
        assert_eq!(names("echo \"$(ssh host)\""), ["ssh", "echo"]);
        assert_eq!(names("echo \"`ssh host`\""), ["ssh", "echo"]);
        assert_eq!(words_of("\"a\\\nb\" y"), ["ab"]);
    }

    #[test]
    fn unterminated_quote_is_an_error() {
        for command in [
            "echo 'a",
            "echo \"a",
            "echo $'a",
            "echo \"$(ls\"",
            "echo `ls",
        ] {
            assert!(lex(command).is_err(), "{command}");
        }
        assert_eq!(lex("echo 'a").unwrap_err(), LexError::UnterminatedQuote);
        assert_eq!(lex("echo \"a").unwrap_err(), LexError::UnterminatedQuote);
        assert_eq!(
            lex("echo `ls").unwrap_err(),
            LexError::UnterminatedSubstitution
        );
        assert_eq!(
            lex("echo $(ls").unwrap_err(),
            LexError::UnterminatedSubstitution
        );
        assert_eq!(
            lex("echo ${x").unwrap_err(),
            LexError::UnterminatedExpansion
        );
    }

    #[test]
    fn oversized_input_is_an_error() {
        let at_limit = "a".repeat(MAX_INPUT_BYTES);
        assert!(lex(&at_limit).is_ok());
        let over = "a".repeat(MAX_INPUT_BYTES + 1);
        assert_eq!(lex(&over).unwrap_err(), LexError::TooLong);
    }

    #[test]
    fn depth_over_three_is_an_error() {
        let nest = |levels: usize| {
            let mut text = "ls".to_owned();
            for _ in 0..levels {
                text = format!("echo $({text})");
            }
            text
        };
        assert!(lex(&nest(MAX_DEPTH)).is_ok());
        assert_eq!(lex(&nest(MAX_DEPTH + 1)).unwrap_err(), LexError::TooDeep);
        assert_eq!(
            lex("bash -c 'bash -c \"bash -c \\\"bash -c ls\\\"\"'").unwrap_err(),
            LexError::TooDeep
        );
        assert!(lex("bash -c 'bash -c \"bash -c ls\"'").is_ok());
    }

    #[test]
    fn nesting_is_bounded() {
        let deep = format!(
            "echo {}x{}",
            "${a:-".repeat(MAX_NESTING + 1),
            "}".repeat(MAX_NESTING + 1)
        );
        assert_eq!(lex(&deep).unwrap_err(), LexError::TooNested);
        let fine = format!("echo {}x{}", "${a:-".repeat(8), "}".repeat(8));
        assert!(lex(&fine).is_ok());
    }

    #[test]
    fn joined_words_equal_what_bash_runs() {
        for command in [
            "s\"s\"h host",
            "\"ss\"h host",
            "s'sh' host",
            "\\ssh host",
            "ss\\\nh host",
            "''ssh host",
        ] {
            let lexed = lex(command).unwrap();
            assert_eq!(lexed.programs[0].written, "ssh", "{command:?}");
            assert!(lexed.dynamic.is_empty(), "{command:?}");
        }
    }

    #[test]
    fn carriage_returns_are_dropped_like_git_bash() {
        assert_eq!(names("ss\rh host\r\n"), ["ssh"]);
        assert_eq!(names("'ss\rh' host"), ["ssh"]);
    }

    #[test]
    fn line_continuations_join_operators_and_expansions() {
        assert_eq!(names("ls &\\\n& ssh host"), ["ls", "ssh"]);
        assert_eq!(names("echo $\\\n(ssh host)"), ["ssh", "echo"]);
    }

    #[test]
    fn comments_end_at_the_line_break() {
        assert_eq!(names("echo a # ssh host\nls"), ["echo", "ls"]);
        assert_eq!(names("echo a#b; ssh host"), ["echo", "ssh"]);
    }

    #[test]
    fn heredoc_bodies_are_data_unless_expanded() {
        assert_eq!(names("cat <<EOF\nssh host\nEOF\nls"), ["cat", "ls"]);
        assert_eq!(names("cat <<'EOF'\n$(ssh host)\nEOF"), ["cat"]);
        assert_eq!(names("cat <<EOF\n$(ssh host)\nEOF"), ["ssh", "cat"]);
        assert_eq!(names("cat <<-EOF\n\tx\n\tEOF\nls"), ["cat", "ls"]);
    }

    #[test]
    fn heredoc_delimiter_is_compared_after_joining_continuations() {
        assert_eq!(
            names("cat <<EOF\nhello\nEO\\\nF\nssh host\nEOF"),
            ["cat", "ssh", "eof"]
        );
        assert_eq!(names("cat <<'EOF'\nEO\\\nF\nssh host\nEOF"), ["cat"]);
    }

    #[test]
    fn arithmetic_is_not_a_command_or_a_heredoc() {
        assert_eq!(names("(( x << 2 )) && ls"), ["ls"]);
        assert_eq!(names("echo $((1 << 2))"), ["echo"]);
        assert_eq!(names("echo $((ssh host) )"), ["ssh", "echo"]);
        assert_eq!(names("((ssh host) )"), ["ssh"]);
        assert_eq!(names("for ((i=0; i<3; i++)); do ssh h; done"), ["ssh"]);
    }

    #[test]
    fn case_patterns_are_not_commands() {
        assert_eq!(
            names("case x in ssh) ls;; (b|c) ssh h;; esac"),
            ["ls", "ssh"]
        );
        assert_eq!(names("echo $(case x in x) ssh h;; esac)"), ["ssh", "echo"]);
    }

    #[test]
    fn steps_count_every_level() {
        let flat = lex("echo a").unwrap().steps;
        let nested = lex("echo $(echo $(echo a))").unwrap().steps;
        assert!(nested > flat);
    }

    /// One unit of a shape that stresses every construct the lexer nests:
    /// quotes, substitutions three deep, a shell's text, ANSI-C quoting, an
    /// array, a case, continuations and a here-document.
    const STRESS_UNIT: &str = "x=(a \"b\" $'c\\x41') ; echo \"$(printf '%s' \"$(true \"a\\\"b\" `echo q`)\")\" 's\"q' \\\n | bash -c 'ls \"$(pwd)\"' ; case $x in a|b) true;; esac ; cat <<'E'\nbody $(not) line\nE\n";

    fn stress(bytes: usize) -> String {
        let mut text = String::with_capacity(bytes + STRESS_UNIT.len());
        while text.len() + STRESS_UNIT.len() <= bytes {
            text.push_str(STRESS_UNIT);
        }
        text
    }

    #[test]
    fn step_count_grows_linearly() {
        let largest = stress(MAX_INPUT_BYTES);
        assert!(lex(&largest).is_ok());
        let small = lex(&stress(64 * 1024)).unwrap().steps;
        let large = lex(&stress(128 * 1024)).unwrap().steps;
        let ratio = large as f64 / small as f64;
        assert!(
            ratio < 2.2,
            "128 KiB took {large} steps, 64 KiB {small}: ratio {ratio}"
        );
        assert!(
            ratio > 1.8,
            "the two inputs must have the same shape: ratio {ratio}"
        );
    }

    #[test]
    fn written_and_program_names() {
        assert_eq!(program_name("/usr/bin/ssh"), "ssh");
        assert_eq!(program_name(r"C:\Windows\System32\OpenSSH\ssh.exe"), "ssh");
        assert_eq!(program_name("SSH.EXE"), "ssh");
        assert_eq!(program_name("ssh.exe. "), "ssh");
        assert_eq!(program_name("ssh.exe::$DATA"), "ssh");
        assert_eq!(program_name("ſsh"), "ssh");
        assert_eq!(written_name("C:/Windows/System32/BASH.EXE"), "bash.exe");
        assert!(under_windows_dir("/c/WINDOWS/sysnative/bash"));
        assert!(!under_windows_dir("/usr/bin/bash"));
        assert!(name_matches("ansible*", "Ansible-Playbook"));
        assert!(!name_matches("ssh", "ssh-keygen"));
    }

    #[test]
    fn every_encoded_spelling_is_recognised() {
        for spelling in encoded_command_spellings() {
            for prefix in ["-", "--", "/"] {
                let arg = format!("{prefix}{}", spelling.to_uppercase());
                assert_eq!(encoded_command_parameter(&arg), Some(spelling), "{arg}");
            }
        }
        assert_eq!(encoded_command_parameter("-ExecutionPolicy"), None);
        assert_eq!(encoded_command_parameter("-"), None);
        assert_eq!(encoded_command_parameter("enc"), None);
    }

    #[test]
    fn every_builtin_name_resolves_to_itself() {
        for builtin in Builtin::ALL {
            assert_eq!(Builtin::from_name(builtin.name()), Some(*builtin));
        }
    }

    #[test]
    fn command_bindings_bind_a_name_to_a_program() {
        assert_eq!(names("BASH_CMDS[f]=/usr/bin/ssh; f host01"), ["ssh", "f"]);
        assert_eq!(
            names("BASH_ALIASES[s]='ssh host01'\ns"),
            ["ssh", "s"],
            "the alias value is lexed, and the alias name is still a word"
        );
        assert_eq!(
            lex("BASH_CMDS[f]=$TOOL; f host01").unwrap().dynamic.len(),
            1
        );
        let assigns = lex("BASH_CMDS[f]=/usr/bin/ssh").unwrap().assignments;
        assert_eq!(assigns, [Assignment::Name("BASH_CMDS".into())]);
    }

    #[test]
    fn latent_substitutions_are_reported_alone() {
        assert_eq!(names("PS4='$(ssh host)' bash -xc :"), ["ssh", "bash", ":"]);
        assert_eq!(names("awk '{print $(NF)}' f"), ["awk", "nf"]);
        let lexed = lex("awk '{ x = $($1) }' f").unwrap();
        assert!(lexed.dynamic.is_empty());
        assert!(lex("sed 's/`x/y/' f").is_ok());
    }
}
