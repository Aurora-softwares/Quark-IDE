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
            if (handler == "LoadSession") {
                Trace("0: LoadSession entered");
                string kind = Session.Kind();
                Trace("1: Kind read: " + kind);
                string selected = Session.Path();
                Trace("2: Path read");
                if (kind == "P") { LoadProject(args, selected); }
                else if (kind == "S") { LoadDocument(args, selected, false); }
                else if (kind == "U") { LoadDocument(args, selected, true); }
                else if (kind == "F") {
                    Application.SetText("projectTitle", Workspace.BaseName(selected));
                    Application.SetText("projectPathLabel", selected);
                    Application.SetText("projectFileList", "Folder opened. Choose a source file or .hyproj with the file pickers.");
                    Application.SetText("propertyName", Workspace.BaseName(selected));
                    Application.SetText("propertyType", "Folder workspace (file enumeration not implemented)");
                    Status("Opened folder " + selected);
                } else { Status("Empty workspace. Open a project, source file or HYUI document."); }
                Trace("6: About to clear request");
                Session.Clear();
                Trace("7: Session consumed");
            }
            if (handler == "BackToStart") {
                if (Dirty(args)) { Status("Save your changes before returning to Start."); }
                else if (Session.OpenStart()) { Application.Exit(); }
                else { Status("Cannot open Start. Build both applications from the repository root."); }
            }
            if (handler == "OpenProjectHelp") { Status("Use Open project… in Solution Explorer."); }
            if (handler == "SelectTool") {
                Application.SetText("propertyName", "Hydrogen UI component #" + Application.Value(args, "toolboxItems"));
                Application.SetText("propertyType", "Visual drag/drop editing is not implemented yet.");
            }
            if (handler == "ShowCode") { ShowEditor(0); }
            if (handler == "ShowUI") { ShowEditor(1); }
            if (handler == "ShowOutput") { Application.SetValue("bottomTabs", 0); }
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
                string codePath = Application.Value(args, "codePath");
                string layoutPath = Application.Value(args, "uiPath");
                if ((codePath == "" && Application.Value(args, "codeText") != "") ||
                    (layoutPath == "" && Application.Value(args, "uiText") != "")) {
                    Status("A changed untitled buffer needs Save as… first.");
                } else {
                    if (codePath != "") { Save(args, codePath, false); }
                    if (layoutPath != "") { Save(args, layoutPath, true); }
                }
            }
            if (handler == "ExploreSelected") {
                string project = Application.Value(args, "projectState");
                string manifest = Application.Value(args, "manifestState");
                if (project != "") {
                    int chosen = System.Convert.ToInt32(Application.Value(args, "projectFileNumber")) - 1;
                    if (chosen < 0) { Status("Choose file number 1 or higher."); return 0; }
                    int count = Workspace.SourceCount(project, manifest);
                    if (chosen < count) {
                        LoadDocument(args, Workspace.SourceAt(project, manifest, chosen), false);
                    } else if (chosen == count && Workspace.Layout(project, manifest) != "") {
                        LoadDocument(args, Workspace.Layout(project, manifest), true);
                    } else {
                        Application.SetText("output", Workspace.Preview(manifest));
                        Application.SetText("propertyName", Workspace.BaseName(project));
                        Application.SetText("propertyType", "Hydrogen project (.hyproj)");
                    }
                }
            }
            if (handler == "ProjectInfo") {
                string manifest = Application.Value(args, "manifestState");
                if (manifest == "") { Status("Open a project first."); }
                else {
                    Application.SetText("output", Workspace.Preview(manifest));
                    Application.SetValue("bottomTabs", 0);
                    Application.SetText("propertyType", "Hydrogen format 2 project manifest");
                }
            }
            if (handler == "ReplaceFirst" || handler == "ReplaceAll") {
                bool onUI = Application.Value(args, "editorTabs") == "1";
                string field = onUI ? "uiText" : "codeText";
                string data = Application.Value(args, field);
                string term = Application.Value(args, "searchQuery");
                string replacement = Application.Value(args, "replaceQuery");
                if (term == "") {
                    Application.SetText("searchResult", "Enter a non-empty search term.");
                } else {
                    int count = Workspace.CountMatches(data, term);
                    if (count == 0) {
                        Application.SetText("searchResult", "No match in the open document.");
                    } else {
                        bool all = handler == "ReplaceAll";
                        string updated = all ? Workspace.ReplaceAll(data, term, replacement) :
                            Workspace.ReplaceFirst(data, term, replacement);
                        Application.SetText(field, updated);
                        int changed = all ? count : 1;
                        Application.SetText("searchResult", "Replaced " + changed + " occurrence(s). Save to keep your edits.");
                        Status("Document modified (" + changed + " replacement(s)); save before building.");
                    }
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
                Application.SetValue("bottomTabs", 0);
            }
            return 0;
        }

        private static void Trace(string step) {
            // Emit a short marker before file I/O so a signal-terminated
            // backend still tells us which step executed.
            System.Console.WriteLine("QUARK_SESSION_TRACE: " + step);
            System.IO.File.WriteAllText("build/.quark-onload.trace", step + "\n");
        }

        private static void ShowEditor(int page) {
            Application.SetValue("editorTabs", page);
            Application.SetText("fileMode", page == 0 ? "Hydrogen source" : "Hydrogen UI XML");
        }

        private static void Status(string message) {
            Application.SetText("status", message);
        }

        private static bool Dirty(string[] args) {
            string path = Application.Value(args, "codePath");
            string text = Application.Value(args, "codeText");
            if (path == "") {
                if (text != "") { return true; }
            } else if (!System.IO.File.Exists(path) || System.IO.File.ReadAllText(path) != text) { return true; }
            path = Application.Value(args, "uiPath");
            text = Application.Value(args, "uiText");
            if (path == "") {
                if (text != "") { return true; }
            } else if (!System.IO.File.Exists(path) || System.IO.File.ReadAllText(path) != text) { return true; }
            return false;
        }

        private static void LoadProject(string[] args, string path) {
            Trace("3a: LoadProject entered");
            if (path == "" || !System.IO.File.Exists(path)) { Status("Selected project does not exist."); return; }
            if (Dirty(args)) { Status("Save all changed documents before opening another project."); return; }
            Trace("3b: Project validated");
            string manifest = System.IO.File.ReadAllText(path);
            Trace("3c: Manifest read");
            string code = Workspace.Source(path, manifest);
            string layout = Workspace.Layout(path, manifest);
            Trace("3d: Source and UI paths resolved");
            Application.SetText("projectState", path);
            Application.SetText("manifestState", manifest);
            Application.SetText("projectTitle", Workspace.QuotedSetting(manifest, "name"));
            Application.SetText("startupProject", Workspace.QuotedSetting(manifest, "name"));
            Application.SetText("projectPathLabel", path);
            Application.SetText("projectFileList", Workspace.ExplorerItems(path, manifest));
            Trace("3e: Explorer generated");
            Application.SetValue("projectFileNumber", 1);
            Application.SetText("propertyName", Workspace.BaseName(path));
            Application.SetText("propertyType", "Hydrogen project (.hyproj)");
            Application.SetText("codePath", code);
            Application.SetText("uiPath", layout);
            Trace("3f: Before code buffer");
            Application.SetText("codeText", code != "" && System.IO.File.Exists(code) ? System.IO.File.ReadAllText(code) : "");
            Trace("3g: Code buffer serialized");
            Application.SetText("uiText", layout != "" && System.IO.File.Exists(layout) ? System.IO.File.ReadAllText(layout) : "");
            Trace("3h: UI buffer serialized");
            Application.SetText("codeName", code == "" ? "No source in manifest" : Workspace.BaseName(code));
            Application.SetText("uiName", layout == "" ? "No HYUI layout in manifest" : Workspace.BaseName(layout));
            Application.SetText("documentTitle", Workspace.BaseName(path));
            ShowEditor(0);
            Status("Opened " + path);
            Trace("3i: LoadProject completed");
        }

        private static void LoadDocument(string[] args, string path, bool ui) {
            if (path == "" || !System.IO.File.Exists(path)) { Status("Selected file does not exist."); return; }
            if (Dirty(args)) { Status("Save all changed documents before opening another file."); return; }
            if (ui) {
                Application.SetText("uiPath", path);
                Application.SetText("uiText", System.IO.File.ReadAllText(path));
                Application.SetText("uiName", Workspace.BaseName(path));
                ShowEditor(1);
            } else {
                Application.SetText("codePath", path);
                Application.SetText("codeText", System.IO.File.ReadAllText(path));
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
            System.IO.File.WriteAllText(path, content);
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
