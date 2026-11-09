using System;
using System.IO;

namespace Quark {
    public class CommandResult {
        private int exitCode;
        private string captured;
        public CommandResult(int code, string output) { exitCode = code; captured = output; }
        public int Code() { return exitCode; }
        public string Text() { return captured + "\n\nExit code: " + exitCode; }
    }

    public class BuildService {
        public static string Quote(string arg) {
            if (arg.Length == 0) { return ""; }
            int i = 0;
            if (System.Host.Process.IsWindows()) {
                while (i < arg.Length) {
                    // cmd.exe re-expands these even in some quoted contexts.
                    string c = arg[i];
                    if (c == "\"" || c == "%" || c == "!" || c == "&" || c == "|" ||
                        c == "<" || c == ">" || c == "^" || c == "\r" || c == "\n") { return ""; }
                    i = i + 1;
                }
                return "\"" + arg + "\"";
            }
            string quoted = "'";
            while (i < arg.Length) {
                if (arg[i] == "\r" || arg[i] == "\n") { return ""; }
                if (arg[i] == "'") { quoted = quoted + "'\\''"; }
                else { quoted = quoted + arg[i]; }
                i = i + 1;
            }
            return quoted + "'";
        }

        private static CommandResult RunLogged(string command, string logPath) {
            string log = Quote(logPath);
            if (log == "") { return new CommandResult(127, "Unsafe log path. Nothing was executed."); }
            int exitCode = System.Host.Process.Run(command + " > " + log + " 2>&1");
            string output = "";
            if (System.IO.File.Exists(logPath)) { output = Workspace.Preview(System.IO.File.ReadAllText(logPath)); }
            if (output == "") { output = "(No command output captured)"; }
            return new CommandResult(exitCode, output);
        }

        public static string Execute(string compiler, string project, string action, bool hasUI, string target) {
            if (!System.IO.File.Exists(project)) { return "No existing .hyproj selected."; }
            if (target != "host" && target != "macos-arm64") { return "Unknown application build target."; }
            if (target == "macos-arm64" &&
                (System.Host.Process.IsWindows() ||
                 System.Host.Process.Run("test \"$(uname -s)\" = Darwin") != 0)) {
                return "macOS arm64 builds require macOS and Xcode Command Line Tools.";
            }
            string command = Quote(compiler);
            string projectArg = Quote(project);
            string output = project + ".quark-build";
            string outputArg = Quote(output);
            string log = project + ".quark-command.log";
            if (command == "" || projectArg == "" || outputArg == "") {
                return "Compiler or project path contains unsupported shell characters. No command was executed.";
            }
            if (action == "Check") {
                return RunLogged(command + " check " + projectArg, log).Text();
            }
            if (action == "Preview") {
                if (!hasUI) { return "This project has no .hyui entry. Open an HYUI application project first."; }
                // Hydrogen owns the native preview build and host-specific runtime.
                string preview = RunLogged(command + " ui " + projectArg, log).Text();
                if (target == "macos-arm64") { return "Preview uses the host UI runtime.\n" + preview; }
                return preview;
            }
            string targetArg = target == "macos-arm64" ? " --target macos-arm64" : "";
            CommandResult built = RunLogged(command + " build " + projectArg + targetArg + " -o " + outputArg, log);
            if (built.Code() != 0) { return "Build failed.\n" + built.Text(); }
            if (action == "Build") { return "Build completed.\n" + built.Text(); }
            if (action == "Run") {
                if (target == "macos-arm64") {
                    if (System.Host.Process.Run("test \"$(uname -m)\" = arm64") != 0) {
                        return "Built macOS arm64 output. Run it on Apple Silicon.\n" + built.Text();
                    }
                    if (hasUI) {
                        string bundle = output + ".app";
                        string bundleArg = Quote(bundle);
                        if (bundleArg == "" || System.Host.Process.Run("test -d " + bundleArg) != 0) {
                            return "Compiler reported success, but the arm64 app bundle was not found.\n" + built.Text();
                        }
                        return "Build output:\n" + built.Text() + "\n\nLaunch result:\n" +
                            RunLogged("open " + bundleArg, log).Text();
                    }
                }
                if (hasUI) {
                    // The native host is launched through Hydrogen's packaging pipeline.
                    return RunLogged(command + " ui " + projectArg, log).Text();
                }
                string executable = output;
                if (System.Host.Process.IsWindows()) { executable = executable + ".exe"; }
                if (!System.IO.File.Exists(executable)) {
                    return "Compiler reported success, but the host executable could not be found: " + executable + "\n" + built.Text();
                }
                string executableArg = Quote(executable);
                if (executableArg == "") { return "Unsafe executable path; cannot run.\n" + built.Text(); }
                return "Build output:\n" + built.Text() + "\n\nProgram output:\n" + RunLogged(executableArg, log).Text();
            }
            return "Unknown build action.";
        }
    }
}
