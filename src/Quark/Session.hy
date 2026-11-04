using System;
using System.IO;

namespace Quark {
    // The two HYUI applications do not share a process. Persist only the
    // requested project location (not document contents) during hand-off.
    public class Session {
        public static string RequestFile() { return "build/.quark-open-request"; }
        public static string RecentFile() { return "build/.quark-recent"; }

        public static string Kind() {
            if (!System.IO.File.Exists(RequestFile())) { return ""; }
            string text = System.IO.File.ReadAllText(RequestFile());
            if (text.Length < 3 || text[1] != "\n") { return ""; }
            return text[0];
        }

        public static string Path() {
            if (!System.IO.File.Exists(RequestFile())) { return ""; }
            string text = System.IO.File.ReadAllText(RequestFile());
            if (text.Length < 3 || text[1] != "\n") { return ""; }
            int end = 2;
            while (end < text.Length && text[end] != "\n" && text[end] != "\r") { end = end + 1; }
            return Workspace.Slice(text, 2, end - 2);
        }

        public static void Clear() {
            if (System.IO.File.Exists(RequestFile())) { System.IO.File.WriteAllText(RequestFile(), ""); }
        }

        public static string Quote(string value) {
            if (value == "") { return ""; }
            int i = 0;
            if (System.Host.Process.IsWindows()) {
                while (i < value.Length) {
                    string c = value[i];
                    if (c == "\"" || c == "%" || c == "!" || c == "&" ||
                        c == "|" || c == "<" || c == ">" || c == "^" || c == "\n" || c == "\r") { return ""; }
                    i = i + 1;
                }
                return "\"" + value + "\"";
            }
            string result = "'";
            while (i < value.Length) {
                string c = value[i];
                if (c == "\n" || c == "\r") { return ""; }
                if (c == "'") { result = result + "'\\''"; }
                else { result = result + c; }
                i = i + 1;
            }
            return result + "'";
        }

        private static int Spawn(string target) {
            if (System.Host.Process.IsWindows()) {
                string executable = "build/" + target + ".exe";
                if (!System.IO.File.Exists(executable)) { return 127; }
                return System.Host.Process.Run("start \"\" " + Quote(executable));
            }
            if (System.Host.Process.Run("test \"$(uname -s)\" = Darwin") == 0) {
                string bundle = "build/" + target + ".app";
                if (System.Host.Process.Run("test -d " + Quote(bundle)) != 0) { return 127; }
                return System.Host.Process.Run("open " + Quote(bundle));
            }
            string executable = "./build/" + target;
            if (!System.IO.File.Exists("build/" + target)) { return 127; }
            // Detach the second native window. We may close this one afterwards.
            // Keep native host/backend diagnostics instead of throwing away
            // the exact reason for a failed second-window launch.
            string logFile = target == "QuarkIDE" ? "build/.quark-ide.log" : "build/.quark-start.log";
            return System.Host.Process.Run("nohup " + Quote(executable) +
                " > " + Quote(logFile) + " 2>&1 </dev/null &");
        }

        private static bool ValidRequest(string kind, string path) {
            if (kind == "E") { return path == ""; }
            if (path.Length == 0 || path.Length > 2048) { return false; }
            int i = 0;
            while (i < path.Length) {
                if (path[i] == "\r" || path[i] == "\n") { return false; }
                i = i + 1;
            }
            if (kind == "F") { return true; } // Selected with a native folder picker.
            if (kind == "P" || kind == "S" || kind == "U") { return System.IO.File.Exists(path); }
            return false;
        }

        public static bool OpenIDE(string kind, string path) {
            if (!ValidRequest(kind, path)) { return false; }
            // Avoid changing the session if no companion workspace was built.
            bool built = System.IO.File.Exists("build/QuarkIDE") ||
                         System.IO.File.Exists("build/QuarkIDE.exe");
            if (!built && System.Host.Process.Run("test -d build/QuarkIDE.app") != 0) { return false; }
            System.IO.File.WriteAllText(RequestFile(), kind + "\n" + path + "\n");
            if (Spawn("QuarkIDE") != 0) {
                Clear();
                return false;
            }
            if (kind == "P" || kind == "S" || kind == "U" || kind == "F") { Remember(path); }
            return true;
        }

        public static bool OpenStart() { return Spawn("Quark") == 0; }

        public static string RecentAt(int wanted) {
            if (wanted < 0 || wanted >= 10 || !System.IO.File.Exists(RecentFile())) { return ""; }
            string text = System.IO.File.ReadAllText(RecentFile());
            int pos = 0;
            int item = 0;
            while (pos < text.Length) {
                int end = pos;
                while (end < text.Length && text[end] != "\n" && text[end] != "\r") { end = end + 1; }
                if (item == wanted) { return Workspace.Slice(text, pos, end - pos); }
                while (end < text.Length && (text[end] == "\n" || text[end] == "\r")) { end = end + 1; }
                pos = end;
                item = item + 1;
            }
            return "";
        }

        public static string RecentItems(string filter) {
            string result = "";
            int i = 0;
            while (i < 10) {
                string path = RecentAt(i);
                if (path != "" && (filter == "" || Workspace.FindLine(path, filter) > 0)) {
                    result = result + (i + 1) + ". " + Workspace.BaseName(path) + "\n    " + path + "\n";
                }
                i = i + 1;
            }
            if (result == "") { return "No recent projects match your search."; }
            return result;
        }

        public static void Remember(string path) {
            if (path == "" || path.Length > 2048) { return; }
            string combined = path + "\n";
            int i = 0;
            int count = 1;
            while (i < 10 && count < 10) {
                string old = RecentAt(i);
                if (old != "" && old != path) {
                    combined = combined + old + "\n";
                    count = count + 1;
                }
                i = i + 1;
            }
            System.IO.File.WriteAllText(RecentFile(), combined);
        }

        public static void Forget(int selected) {
            string updated = "";
            int i = 0;
            while (i < 10) {
                string path = RecentAt(i);
                if (path != "" && i != selected) { updated = updated + path + "\n"; }
                i = i + 1;
            }
            System.IO.File.WriteAllText(RecentFile(), updated);
        }

        public static bool GithubURL(string url) {
            string a = "https://github.com/";
            string b = "git@github.com:";
            bool allowed = false;
            if (url.Length > a.Length && Workspace.Slice(url, 0, a.Length) == a) { allowed = true; }
            if (url.Length > b.Length && Workspace.Slice(url, 0, b.Length) == b) { allowed = true; }
            if (!allowed) { return false; }
            int i = 0;
            while (i < url.Length) {
                string c = url[i];
                if (!SafeGitChar(c)) { return false; }
                i = i + 1;
            }
            return true;
        }

        private static bool SafeGitChar(string needle) {
            string allowed = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-._/:@";
            int i = 0;
            while (i < allowed.Length) {
                if (needle == allowed[i]) { return true; }
                i = i + 1;
            }
            return false;
        }
    }
}
