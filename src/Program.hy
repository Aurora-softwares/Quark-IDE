using System;
using System.IO;
using Hydrogen.UI;

namespace Quark {
    public class Program {
        public static int Main(string[] args) {
            if (!Application.IsEvent(args)) {
                Application.Open("Quark.hyui");
                return 0;
            }
            string handler = Application.Handler(args);
            if (handler == "ShowWelcome") { Application.SetValue("screen", 0); }
            if (handler == "Continue") { Application.SetValue("screen", 1); }
            if (handler == "ShowCode") { ShowEditor(0); }
            if (handler == "ShowUI") { ShowEditor(1); }
            if (handler == "ShowOutput") { Application.SetValue("screen", 1); }
            if (handler == "About") { Application.SetOpen("aboutDialog", true); }
            if (handler == "PickerCancelled") { Status("File selection cancelled."); }

            if (handler == "ProjectPicked") {
                LoadProject(args, Application.Value(args, Application.Sender(args)));
            }
            if (handler == "SourcePicked") {
                LoadDocument(args, Application.Value(args, Application.Sender(args)), false);
            }
            if (handler == "UIPicked") {
                LoadDocument(args, Application.Value(args, Application.Sender(args)), true);
            }
            if (handler == "CreateProject") {
                if (Dirty(args)) { Status("Save current edits before creating another project."); return 0; }
                string folder = Application.Value(args, Application.Sender(args));
                string name = Application.Value(args, "newName");
                bool withUI = Application.Value(args, "newUI") == "true";
                string project = Workspace.Create(folder, name, withUI);
                if (project == "") {
                    Status("Could not create project. Check the name and choose a folder without conflicting files.");
                } else {
                    LoadProject(args, project);
                    Status("Created " + project);
                }
            }
            if (handler == "NewSource") {
                if (Dirty(args)) { Status("Save edits before creating a new buffer."); return 0; }
                Application.SetText("codePath", "");
                Application.SetText("codeName", "Untitled .hy — use Save as…");
                Application.SetText("codeText", "");
                Application.SetText("documentTitle", "Untitled Hydrogen source");
                ShowEditor(0);
            }
            if (handler == "SourceSaveAsPicked") {
                Save(args, Application.Value(args, Application.Sender(args)), false);
            }
            if (handler == "UISaveAsPicked") {
                Save(args, Application.Value(args, Application.Sender(args)), true);
            }
            if (handler == "SaveSource") { Save(args, Application.Value(args, "codePath"), false); }
            if (handler == "SaveUI") { Save(args, Application.Value(args, "uiPath"), true); }
            if (handler == "SaveActive") {
                if (Application.Value(args, "editorTabs") == "1") {
                    Save(args, Application.Value(args, "uiPath"), true);
                } else {
                    Save(args, Application.Value(args, "codePath"), false);
                }
            }
            if (handler == "SaveAll") {
                if (Application.Value(args, "codePath") == "" || Application.Value(args, "uiPath") == "") {
                    Status("Save all requires named documents. Use Save as… for new buffers.");
                } else {
                    Save(args, Application.Value(args, "codePath"), false);
                    Save(args, Application.Value(args, "uiPath"), true);
                }
            }
            if (handler == "ProjectInfo") {
                string manifest = Application.Value(args, "manifestState");
                if (manifest == "") { Status("Open a project first."); }
                else {
                    Application.SetText("output", Workspace.Preview(manifest));
                    Application.SetValue("screen", 1);
                    Application.SetText("propertyType", "Hydrogen format 2 project manifest");
                }
            }
            if (handler == "Find") {
                bool onUI = Application.Value(args, "editorTabs") == "1";
                string data = Application.Value(args, onUI ? "uiText" : "codeText");
                string term = Application.Value(args, "searchQuery");
                int line = Workspace.FindLine(data, term);
                if (term == "") { Application.SetText("searchResult", "Enter a search term."); }
                else if (line == 0) { Application.SetText("searchResult", "No match in the open document."); }
                else { Application.SetText("searchResult", "First match on line " + line + " (no jump-to-line API yet)."); }
            }
            if (handler == "Check" || handler == "Build" || handler == "Run" || handler == "Preview") {
                if (Dirty(args)) {
                    Status("Unsaved edits. Save your documents before building or previewing.");
                    return 0;
                }
                string projectPath = Application.Value(args, "projectState");
                string compiler = Application.Value(args, "compiler");
                if (compiler == "") { Status("Enter a compiler executable in Properties."); return 0; }
                string manifest = Application.Value(args, "manifestState");
                bool hasUI = Workspace.QuotedSetting(manifest, "ui") != "";
                string result = BuildService.Execute(compiler, projectPath, handler, hasUI);
                Application.SetText("output", result);
                Application.SetText("status", handler + " finished — see Output for the command result.");
                Application.SetValue("screen", 1);
            }
            return 0;
        }

        private static void ShowEditor(int page) {
            Application.SetValue("screen", 1);
            Application.SetValue("editorTabs", page);
        }

        private static void Status(string message) {
            Application.SetText("status", message);
            Application.SetText("startStatus", message);
        }

        private static bool Dirty(string[] args) {
            string path = Application.Value(args, "codePath");
            string text = Application.Value(args, "codeText");
            if (path == "") {
                if (text != "") { return true; }
            } else if (!File.Exists(path) || File.ReadAllText(path) != text) { return true; }
            path = Application.Value(args, "uiPath");
            text = Application.Value(args, "uiText");
            if (path == "") {
                if (text != "") { return true; }
            } else if (!File.Exists(path) || File.ReadAllText(path) != text) { return true; }
            return false;
        }

        private static void LoadProject(string[] args, string path) {
            if (path == "" || !File.Exists(path)) { Status("Selected project does not exist."); return; }
            if (Dirty(args)) { Status("Save all changed documents before opening another project."); return; }
            string manifest = File.ReadAllText(path);
            string code = Workspace.Source(path, manifest);
            string layout = Workspace.Layout(path, manifest);
            Application.SetText("projectState", path);
            Application.SetText("manifestState", manifest);
            Application.SetText("projectTitle", Workspace.QuotedSetting(manifest, "name"));
            Application.SetText("projectPathLabel", path);
            Application.SetText("propertyName", Workspace.BaseName(path));
            Application.SetText("propertyType", "Hydrogen project (.hyproj)");
            Application.SetText("codePath", code);
            Application.SetText("uiPath", layout);
            Application.SetText("codeText", code != "" && File.Exists(code) ? File.ReadAllText(code) : "");
            Application.SetText("uiText", layout != "" && File.Exists(layout) ? File.ReadAllText(layout) : "");
            Application.SetText("codeName", code == "" ? "No source in manifest" : Workspace.BaseName(code));
            Application.SetText("uiName", layout == "" ? "No HYUI layout in manifest" : Workspace.BaseName(layout));
            Application.SetText("documentTitle", Workspace.BaseName(path));
            ShowEditor(0);
            Status("Opened " + path);
        }

        private static void LoadDocument(string[] args, string path, bool ui) {
            if (path == "" || !File.Exists(path)) { Status("Selected file does not exist."); return; }
            if (Dirty(args)) { Status("Save all changed documents before opening another file."); return; }
            if (ui) {
                Application.SetText("uiPath", path);
                Application.SetText("uiText", File.ReadAllText(path));
                Application.SetText("uiName", Workspace.BaseName(path));
                ShowEditor(1);
            } else {
                Application.SetText("codePath", path);
                Application.SetText("codeText", File.ReadAllText(path));
                Application.SetText("codeName", Workspace.BaseName(path));
                ShowEditor(0);
            }
            Application.SetText("documentTitle", Workspace.BaseName(path));
            Application.SetText("propertyName", Workspace.BaseName(path));
            Application.SetText("propertyType", ui ? "Hydrogen UI XML" : "Hydrogen source");
            Status("Opened " + path);
        }

        private static void Save(string[] args, string path, bool ui) {
            if (path == "") { Status("Choose Save as… to give this document a path."); return; }
            string content = Application.Value(args, ui ? "uiText" : "codeText");
            // The file picker explicitly authorises an existing destination.
            // Normal saves never overwrite a different path silently.
            File.WriteAllText(path, content);
            if (ui) {
                Application.SetText("uiPath", path);
                Application.SetText("uiName", Workspace.BaseName(path));
            } else {
                Application.SetText("codePath", path);
                Application.SetText("codeName", Workspace.BaseName(path));
            }
            Status("Saved " + path);
        }
    }
}
