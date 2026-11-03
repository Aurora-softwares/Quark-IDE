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
                            while (cursor < end && text[cursor] != "\"") { cursor = cursor + 1; }
                            if (cursor < end) {
                                int first = cursor + 1;
                                cursor = first;
                                while (cursor < end && text[cursor] != "\"") { cursor = cursor + 1; }
                                return Slice(text, first, cursor - first);
                            }
                        }
                    }
                }
                index = end + 1;
            }
            return "";
        }

        public static string Source(string projectPath, string manifest) {
            string source = QuotedSetting(manifest, "sources");
            if (!SafeRelative(source)) { return ""; }
            return Join(Directory(projectPath), source);
        }

        public static string Layout(string projectPath, string manifest) {
            string name = QuotedSetting(manifest, "ui");
            if (!SafeRelative(name)) { return ""; }
            return Join(Directory(projectPath), name);
        }

        public static bool ValidName(string name) {
            if (name.Length == 0 || name.Length > 64) { return false; }
            int i = 0;
            while (i < name.Length) {
                string ch = name[i];
                bool letter = (ch >= "a" && ch <= "z") || (ch >= "A" && ch <= "Z");
                bool number = ch >= "0" && ch <= "9";
                if (!letter && !number && ch != "_") { return false; }
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
            if (File.Exists(project) || File.Exists(source) ||
                (withUI && (File.Exists(layout) || File.Exists(bridge)))) { return ""; }

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
                    "        Console.WriteLine(\"Hello from Hydrogen!\");\n" +
                    "        return 0;\n    }\n}\n";
            }

            // File writes are ordered so a failed partial creation cannot leave a valid manifest.
            File.WriteAllText(source, program);
            if (withUI) {
                File.WriteAllText(layout,
                    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n" +
                    "<Window version=\"1\" title=\"" + name + "\" width=\"800\" height=\"500\" theme=\"system\">\n" +
                    "  <ColumnPanel gap=\"16\">\n" +
                    "    <Label text=\"Made with Hydrogen UI\" role=\"title\" />\n" +
                    "    <Label id=\"message\" text=\"Press the button below.\" />\n" +
                    "    <Button id=\"hello\" text=\"Say hello\" primary=\"true\" onClick=\"Hello\" />\n" +
                    "  </ColumnPanel>\n</Window>\n");
                File.WriteAllText(bridge,
                    "namespace Hydrogen.UI {\n" +
                    "    public class Application {\n" +
                    "        public static void Open(string path) { System.Console.WriteLine(\"HYUI\\tOPEN\\t\" + path); }\n" +
                    "        public static bool IsEvent(string[] args) { return args.Length >= 3 && args[0] == \"--hyui-event\"; }\n" +
                    "        public static string Handler(string[] args) { if (IsEvent(args)) { return args[1]; } return \"\"; }\n" +
                    "        public static void SetText(string id, string value) { System.Console.WriteLine(\"HYUI\\tSET\\t\" + id + \"\\ttext\\t\" + value); }\n" +
                    "    }\n}\n");
            }
            File.WriteAllText(project, manifest);
            return project;
        }
    }
}
