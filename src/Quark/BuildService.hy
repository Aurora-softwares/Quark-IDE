using System;
using System.IO;

namespace Quark {
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

        private static string RunLogged(string command, string logPath) {
            string log = Quote(logPath);
            if (log == "") { return "Unsafe log path. Nothing was executed."; }
            int exitCode = System.Host.Process.Run(command + " > " + log + " 2>&1");
            string output = "";
            if (File.Exists(logPath)) { output = Workspace.Preview(File.ReadAllText(logPath)); }
            if (output == "") { output = "(No command output captured)"; }
            return output + "\n\nExit code: " + exitCode;
        }

        public static string Execute(string compiler, string project, string action, bool hasUI) {
            if (!File.Exists(project)) { return "No existing .hyproj selected."; }
            string command = Quote(compiler);
            string projectArg = Quote(project);
            string output = project + ".quark-build";
            string outputArg = Quote(output);
            string log = project + ".quark-command.log";
            if (command == "" || projectArg == "" || outputArg == "") {
                return "Compiler or project path contains unsupported shell characters. No command was executed.";
            }
            if (action == "Check") {
                return RunLogged(command + " check " + projectArg, log);
            }
            if (action == "Preview") {
                if (!hasUI) { return "This project has no .hyui entry. Open an HYUI application project first."; }
                // Hydrogen owns the native preview build and host-specific runtime.
                return RunLogged(command + " ui " + projectArg, log);
            }
            string built = RunLogged(command + " build " + projectArg + " -o " + outputArg, log);
            // Read the real child result rather than assuming a binary exists.
            if (!File.Exists(output) && !File.Exists(output + ".exe")) {
                return "Build did not produce the expected host executable.\n" + built;
            }
            if (action == "Build") { return "Build completed.\n" + built; }
            if (action == "Run") {
                if (hasUI) {
                    // Run the packaged native window through Hydrogen's runtime.
                    return RunLogged(command + " ui " + projectArg, log);
                }
                string executable = output;
                if (System.Host.Process.IsWindows()) { executable = executable + ".exe"; }
                string executableArg = Quote(executable);
                if (executableArg == "") { return "Unsafe executable path; cannot run.\n" + built; }
                return "Build output:\n" + built + "\n\nProgram output:\n" + RunLogged(executableArg, log);
            }
            return "Unknown build action.";
        }
    }
}
