using System;
using System.IO;

namespace Quark {
    public class Workspace {
        public static string Directory(string path) {
            int i = path.Length - 1;
            while (i >= 0) {
                if (path[i] == "/" || path[i] == "\\") {
                    if (i == 0) { return "/"; }
                    return Slice(path, 0, i);
                }
                i = i - 1;
            }
            return ".";
        }

        public static string BaseName(string path) {
            int i = path.Length - 1;
            while (i >= 0) {
                if (path[i] == "/" || path[i] == "\\") { return Slice(path, i + 1, path.Length - i - 1); }
                i = i - 1;
            }
            return path;
        }

        public static string Join(string folder, string leaf) {
            if (folder == "" || folder == ".") { return leaf; }
            if (folder[folder.Length - 1] == "/" || folder[folder.Length - 1] == "\\") { return folder + leaf; }
            return folder + "/" + leaf;
        }

        public static string Slice(string input, int start, int count) {
            string value = "";
            int i = start;
            while (i < start + count && i < input.Length) { value = value + input[i]; i = i + 1; }
            return value;
        }

        public static bool SafeRelative(string value) {
            if (value.Length == 0 || value[0] == "/" || value[0] == "\\") { return false; }
            int i = 0;
            while (i < value.Length) {
                if (value[i] == ":" || value[i] == "\\") { return false; }
                if (i + 1 < value.Length && value[i] == "." && value[i + 1] == ".") { return false; }
                i = i + 1;
            }
            return true;
        }

        public static string QuotedSetting(string text, string key) {
            int index = 0;
            while (index < text.Length) {
                int end = index;
                while (end < text.Length && text[end] != "\n" && text[end] != "\r") { end = end + 1; }
                int cursor = index;
                while (cursor < end && (text[cursor] == " " || text[cursor] == "\t")) { cursor = cursor + 1; }
                int j = 0;
                while (j < key.Length && cursor + j < end && text[cursor + j] == key[j]) { j = j + 1; }
                if (j == key.Length) {
                    cursor = cursor + j;
                    while (cursor < end && (text[cursor] == " " || text[cursor] == "\t")) { cursor = cursor + 1; }
                    if (cursor < end && text[cursor] == "=") {
                        cursor = cursor + 1;
                        while (cursor < end && (text[cursor] == " " || text[cursor] == "\t")) { cursor = cursor + 1; }
                        if (cursor < end && text[cursor] == "\"") {
                            int first = cursor + 1;
                            cursor = first;
                            while (cursor < end && text[cursor] != "\"") { cursor = cursor + 1; }
                            return Slice(text, first, cursor - first);
                        }
                        if (cursor < end && text[cursor] == "[") {
                            // sources arrays may be formatted over several lines.
                            cursor = cursor + 1;
                            while (cursor < text.Length && text[cursor] != "\"" && text[cursor] != "]") {
                                cursor = cursor + 1;
                            }
                            if (cursor < text.Length && text[cursor] == "\"") {
                                int first = cursor + 1;
                                cursor = first;
                                while (cursor < text.Length && text[cursor] != "\"") { cursor = cursor + 1; }
                                return Slice(text, first, cursor - first);
                            }
                        }
                    }
                }
                index = end + 1;
            }
            return "";
        }

        // Enumerate the source paths declared in the project manifest without requiring
        // a native directory enumeration or a file-system service.
        private static int SourcesArrayStart(string manifest) {
            int index = 0;
            while (index < manifest.Length) {
                int end = index;
                while (end < manifest.Length && manifest[end] != "\n" && manifest[end] != "\r") { end = end + 1; }
                int cursor = index;
                while (cursor < end && (manifest[cursor] == " " || manifest[cursor] == "\t")) { cursor = cursor + 1; }
                string prefix = "sources";
                int j = 0;
                while (j < prefix.Length && cursor + j < end && manifest[cursor + j] == prefix[j]) { j = j + 1; }
                cursor = cursor + j;
                if (j == prefix.Length) {
                    while (cursor < end && (manifest[cursor] == " " || manifest[cursor] == "\t")) { cursor = cursor + 1; }
                    if (cursor < end && manifest[cursor] == "=") {
                        cursor = cursor + 1;
                        while (cursor < end && (manifest[cursor] == " " || manifest[cursor] == "\t")) { cursor = cursor + 1; }
                        if (cursor < end && manifest[cursor] == "[") { return cursor + 1; }
                    }
                }
                index = end + 1;
            }
            return -1;
        }

        public static string SourceAt(string projectPath, string manifest, int wanted) {
            if (wanted < 0 || wanted >= 128) { return ""; }
            int cursor = SourcesArrayStart(manifest);
            if (cursor < 0) { return ""; }
            int current = 0;
            while (cursor < manifest.Length && manifest[cursor] != "]") {
                if (manifest[cursor] == "\"") {
                    int first = cursor + 1;
                    cursor = first;
                    while (cursor < manifest.Length && manifest[cursor] != "\"") { cursor = cursor + 1; }
                    string relative = Slice(manifest, first, cursor - first);
                    if (current == wanted) {
                        if (!SafeRelative(relative)) { return ""; }
                        return Join(Directory(projectPath), relative);
                    }
                    current = current + 1;
                }
                cursor = cursor + 1;
            }
            return "";
        }

        public static int SourceCount(string projectPath, string manifest) {
            int i = 0;
            while (i < 128 && SourceAt(projectPath, manifest, i) != "") { i = i + 1; }
            return i;
        }

        public static string ExplorerItems(string projectPath, string manifest) {
            string items = "";
            int i = 0;
            int count = SourceCount(projectPath, manifest);
            while (i < count) {
                string path = SourceAt(projectPath, manifest, i);
                if (i > 0) { items = items + "|"; }
                items = items + BaseName(path);
                i = i + 1;
            }
            string layout = Layout(projectPath, manifest);
            if (layout != "") {
                if (items != "") { items = items + "|"; }
                items = items + BaseName(layout);
            }
            if (items != "") { items = items + "|"; }
            return items + BaseName(projectPath);
        }

        public static string Source(string projectPath, string manifest) {
            return SourceAt(projectPath, manifest, 0);
        }

        public static string Layout(string projectPath, string manifest) {
            string name = QuotedSetting(manifest, "ui");
            if (!SafeRelative(name)) { return ""; }
            return Join(Directory(projectPath), name);
        }

        private static bool IsIn(string needle, string chars) {
            int i = 0;
            while (i < chars.Length) {
                if (chars[i] == needle) { return true; }
                i = i + 1;
            }
            return false;
        }

        public static bool ValidName(string name) {
            if (name.Length == 0 || name.Length > 64) { return false; }
            int i = 0;
            while (i < name.Length) {
                string ch = name[i];
                bool letter = IsIn(ch, "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ_");
                bool number = IsIn(ch, "0123456789");
                if (!letter && !number) { return false; }
                if (i == 0 && number) { return false; }
                i = i + 1;
            }
            return true;
        }

        public static int FindLine(string body, string needle) {
            if (needle.Length == 0) { return 0; }
            int i = 0;
            int line = 1;
            while (i + needle.Length <= body.Length) {
                int j = 0;
                while (j < needle.Length && body[i + j] == needle[j]) { j = j + 1; }
                if (j == needle.Length) { return line; }
                if (body[i] == "\n") { line = line + 1; }
                i = i + 1;
            }
            return 0;
        }

        public static string Preview(string input) {
            if (input.Length > 12000) { return Slice(input, 0, 12000) + "\n… output truncated at 12,000 characters"; }
            return input;
        }

        // Creates starter files in an existing directory. Never overwrites existing files.
        // Returns an empty string on failure or the new manifest path on success.
        public static string Create(string folder, string name, bool withUI) {
            if (!ValidName(name) || folder.Length == 0) { return ""; }
            string project = Join(folder, name + ".hyproj");
            string source = Join(folder, "Program.hy");
            string layout = Join(folder, "Main.hyui");
            string bridge = Join(folder, "Hydrogen.UI.hy");
            if (System.IO.File.Exists(project) || System.IO.File.Exists(source) ||
                (withUI && (System.IO.File.Exists(layout) || System.IO.File.Exists(bridge)))) { return ""; }

            string manifest = "format = 2\nname = \"" + name + "\"\nversion = \"0.1.0\"\ntype = \"exe\"\n";
            string program = "";
            if (withUI) {
                manifest = manifest + "ui = \"Main.hyui\"\n";
                manifest = manifest + "sources = [\"Program.hy\", \"Hydrogen.UI.hy\"]\n";
                program = "using Hydrogen.UI;\n\npublic class Program {\n" +
                    "    public static int Main(string[] args) {\n" +
                    "        if (!Application.IsEvent(args)) { Application.Open(\"Main.hyui\"); return 0; }\n" +
                    "        if (Application.Handler(args) == \"Hello\") { Application.SetText(\"message\", \"Hello from Hydrogen!\"); }\n" +
                    "        return 0;\n    }\n}\n";
            } else {
                manifest = manifest + "sources = [\"Program.hy\"]\n";
                program = "using System;\n\npublic class Program {\n" +
                    "    public static int Main(string[] args) {\n" +
                    "        System.Console.WriteLine(\"Hello from Hydrogen!\");\n" +
                    "        return 0;\n    }\n}\n";
            }

            // File writes are ordered so a failed partial creation cannot leave a valid manifest.
            System.IO.File.WriteAllText(source, program);
            if (withUI) {
                System.IO.File.WriteAllText(layout,
                    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n" +
                    "<Window version=\"1\" title=\"" + name + "\" width=\"800\" height=\"500\" theme=\"system\">\n" +
                    "  <ColumnPanel gap=\"16\">\n" +
                    "    <Label text=\"Made with Hydrogen UI\" role=\"title\" />\n" +
                    "    <Label id=\"message\" text=\"Press the button below.\" />\n" +
                    "    <Button id=\"hello\" text=\"Say hello\" primary=\"true\" onClick=\"Hello\" />\n" +
                    "  </ColumnPanel>\n</Window>\n");
                System.IO.File.WriteAllText(bridge,
                    "namespace Hydrogen.UI {\n" +
                    "    public class Application {\n" +
                    "        public static void Open(string path) { System.Console.WriteLine(\"HYUI\\tOPEN\\t\" + path); }\n" +
                    "        public static bool IsEvent(string[] args) { return args.Length >= 3 && args[0] == \"--hyui-event\"; }\n" +
                    "        public static string Handler(string[] args) { if (IsEvent(args)) { return args[1]; } return \"\"; }\n" +
                    "        public static void SetText(string id, string value) { System.Console.WriteLine(\"HYUI\\tSET\\t\" + id + \"\\ttext\\t\" + value); }\n" +
                    "    }\n}\n");
            }
            System.IO.File.WriteAllText(project, manifest);
            return project;
        }
    }
}
