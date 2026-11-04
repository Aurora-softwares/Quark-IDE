"""Source contract tests for two independent native Hydrogen windows."""
from pathlib import Path
import re
import unittest
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
START = ET.parse(ROOT / "Quark.Start.hyui").getroot()
IDE = ET.parse(ROOT / "Quark.hyui").getroot()
START_CODE = (ROOT / "src/Quark/StartProgram.hy").read_text()
IDE_CODE = (ROOT / "src/Program.hy").read_text()
SESSION = (ROOT / "src/Quark/Session.hy").read_text()
WORKSPACE = (ROOT / "src/Quark/Workspace.hy").read_text()
BUILD = (ROOT / "src/Quark/BuildService.hy").read_text()
LAUNCH_MANIFEST = (ROOT / "Quark.hyproj").read_text()
IDE_MANIFEST = (ROOT / "Quark.IDE.hyproj").read_text()
BRIDGE = (ROOT / "src/Hydrogen/UI/Application.hy").read_text()
README = (ROOT / "README.md").read_text()

def by_id(doc, name):
    return doc.find(".//*[@id='%s']" % name)

def all_handlers(doc):
    return {node.attrib[key] for node in doc.iter()
            for key in ("onClick", "onPick", "onCancel", "onChange", "onLoad", "onSubmit")
            if key in node.attrib}

class QuarkContracts(unittest.TestCase):
    def test_launcher_is_a_dedicated_small_native_window(self):
        self.assertEqual(START.tag, "Window")
        self.assertIn("Start Window", START.attrib["title"])
        self.assertLess(int(START.attrib["width"]), int(IDE.attrib["width"]))
        self.assertIsNone(by_id(START, "editorTabs"))
        self.assertIsNotNone(by_id(START, "startProjectPicker"))

    def test_ide_is_its_own_window_without_start_tab(self):
        self.assertEqual(IDE.tag, "Window")
        self.assertIn("Development Environment", IDE.attrib["title"])
        self.assertIsNotNone(by_id(IDE, "editorTabs"))
        self.assertIsNotNone(by_id(IDE, "codeText"))
        self.assertIsNotNone(by_id(IDE, "uiText"))
        self.assertIsNone(by_id(IDE, "startProjectPicker"))
        self.assertIsNone(by_id(IDE, "screen"))

    def test_start_actions_have_hydrogen_handlers(self):
        for handler in all_handlers(START):
            with self.subTest(handler=handler):
                self.assertIn('handler == "' + handler + '"', START_CODE)
        ids = [n.attrib["id"] for n in START.iter() if "id" in n.attrib]
        self.assertEqual(len(ids), len(set(ids)))

    def test_ide_actions_have_hydrogen_handlers(self):
        for handler in all_handlers(IDE):
            with self.subTest(handler=handler):
                self.assertIn('handler == "' + handler + '"', IDE_CODE)
        ids = [n.attrib["id"] for n in IDE.iter() if "id" in n.attrib]
        self.assertEqual(len(ids), len(set(ids)))

    def test_both_executables_are_hydrogen_only(self):
        self.assertIn('ui = "Quark.Start.hyui"', LAUNCH_MANIFEST)
        self.assertIn('ui = "Quark.hyui"', IDE_MANIFEST)
        for manifest in (LAUNCH_MANIFEST, IDE_MANIFEST):
            self.assertNotIn(".rs", manifest)
            self.assertIn('src/Hydrogen/UI/Application.hy', manifest)
            self.assertIn('src/Quark/Session.hy', manifest)

    def test_two_native_build_targets_are_built_together(self):
        script = (ROOT / "scripts/build.sh").read_text()
        self.assertIn("hy=\"${HY:-hy}\"", script)
        self.assertIn('build Quark.IDE.hyproj -o build/QuarkIDE', script)
        self.assertIn('build Quark.hyproj -o build/Quark', script)
        self.assertLess(script.index("Quark.IDE.hyproj"), script.index("build Quark.hyproj"))

    def test_hyui_exit_is_supported_in_local_bridge(self):
        self.assertIn("public static void Exit()", BRIDGE)
        self.assertIn("HYUI\\tEXIT", BRIDGE)
        self.assertIn("Application.Exit();", START_CODE)
        self.assertIn("Application.Exit();", IDE_CODE)

    def test_launch_only_exits_after_successful_handoff(self):
        self.assertIn("if (Session.OpenIDE(kind, path))", START_CODE)
        self.assertIn("Application.Exit();", START_CODE)
        self.assertIn("return false;", SESSION)
        self.assertIn("if (Spawn(\"QuarkIDE\") != 0)", SESSION)

    def test_workspace_onload_reads_selection(self):
        self.assertEqual(IDE.attrib.get("onLoad"), "LoadSession")
        self.assertIn('handler == "LoadSession"', IDE_CODE)
        self.assertIn("Session.Kind()", IDE_CODE)
        self.assertIn("Session.Path()", IDE_CODE)
        self.assertIn("Session.Clear()", IDE_CODE)

    def test_ide_can_return_to_start_without_losing_edits(self):
        self.assertEqual(by_id(IDE, "toolbarHome").attrib["onClick"], "BackToStart")
        self.assertIn('handler == "BackToStart"', IDE_CODE)
        self.assertIn('if (Dirty(args))', IDE_CODE)
        self.assertIn("Session.OpenStart()", IDE_CODE)

    def test_native_session_uses_heap_built_paths_for_legacy_runtime(self):
        session = (ROOT / "src/Quark/Session.hy").read_text()
        self.assertIn('Workspace.Join("build", ".quark-open-request")', session)
        self.assertIn('Workspace.Join("build", ".quark-recent")', session)
        self.assertNotIn('return "build/.quark-open-request"', session)

    def test_recent_workspace_history_is_real(self):
        self.assertEqual(START.attrib.get("onLoad"), "StartReady")
        self.assertIn("Session.RecentItems", START_CODE)
        self.assertIn("Session.RecentAt", START_CODE)
        self.assertIn("Session.Forget", START_CODE)
        self.assertIn("public static void Remember", SESSION)
        self.assertIn("Remember(path);", SESSION)

    def test_start_project_and_folder_actions(self):
        self.assertEqual(by_id(START, "startProjectPicker").attrib["onPick"], "ProjectPicked")
        self.assertEqual(by_id(START, "startFolderPicker").attrib["onPick"], "FolderPicked")
        self.assertIn('Launch("P"', START_CODE)
        self.assertIn('Launch("F"', START_CODE)
        self.assertIn('Launch("E", "")', START_CODE)

    def test_start_project_creation_is_connected(self):
        self.assertEqual(by_id(START, "projectFolderPicker").attrib["onPick"], "CreateProject")
        self.assertIn("Workspace.Create(folder, name, ui)", START_CODE)
        self.assertIn('Launch("P", path)', START_CODE)

    def test_starter_creation_does_not_overwrite_files(self):
        self.assertIn("System.IO.File.Exists(project)", WORKSPACE)
        self.assertIn("System.IO.File.Exists(source)", WORKSPACE)
        self.assertIn("System.IO.File.Exists(layout)", WORKSPACE)
        self.assertIn("System.IO.File.Exists(bridge)", WORKSPACE)

    def test_starter_contains_live_hyui_event_handler(self):
        self.assertIn('onClick=\\\"Hello\\\"', WORKSPACE)
        self.assertIn('Application.Handler(args) ==', WORKSPACE)
        self.assertIn('System.IO.File.WriteAllText(project, manifest)', WORKSPACE)

    def test_explorer_lists_source_files_from_manifest(self):
        self.assertEqual(by_id(IDE, "openIndexedFile").attrib["onClick"], "ExploreSelected")
        self.assertIn("Workspace.ExplorerItems(path, manifest)", IDE_CODE)
        self.assertIn('Application.SetText("projectFileList"', IDE_CODE)

    def test_explorer_source_selection_is_connected(self):
        self.assertIn('handler == "ExploreSelected"', IDE_CODE)
        self.assertIn("Workspace.SourceAt(project, manifest, chosen)", IDE_CODE)
        self.assertIn("Workspace.Layout(project, manifest)", IDE_CODE)

    def test_project_manifest_supports_multiline_sources(self):
        self.assertIn('text[cursor] != "]"', WORKSPACE)
        self.assertIn("return SourceAt(projectPath, manifest, 0);", WORKSPACE)
        self.assertIn('manifest[cursor] != "]"', WORKSPACE)

    def test_editor_opens_and_saves_source_documents(self):
        self.assertIn('handler == "SourcePicked"', IDE_CODE)
        self.assertIn("System.IO.File.ReadAllText(path)", IDE_CODE)
        self.assertIn('handler == "SaveSource"', IDE_CODE)
        self.assertIn("System.IO.File.WriteAllText(path, content)", IDE_CODE)

    def test_editor_opens_and_saves_hyui_documents(self):
        self.assertIn('handler == "UIPicked"', IDE_CODE)
        self.assertIn('handler == "SaveUI"', IDE_CODE)
        self.assertIn('Application.SetText("uiText"', IDE_CODE)
        self.assertIn('Application.SetText("uiPath"', IDE_CODE)

    def test_unsaved_documents_block_navigation_and_build(self):
        self.assertIn('Application.Value(args, "codeText")', IDE_CODE)
        self.assertIn('Application.Value(args, "uiText")', IDE_CODE)
        self.assertGreaterEqual(IDE_CODE.count("if (Dirty(args))"), 3)

    def test_compiler_check_build_run_are_real_processes(self):
        self.assertIn("System.Host.Process.Run(", BUILD)
        self.assertIn('command + " check " + projectArg', BUILD)
        self.assertIn('command + " build " + projectArg + " -o " + outputArg', BUILD)
        self.assertIn("if (built.Code() != 0)", BUILD)

    def test_compiler_output_and_preview_are_real(self):
        self.assertIn('log + " 2>&1"', BUILD)
        self.assertIn("System.IO.File.ReadAllText(logPath)", BUILD)
        self.assertIn('command + " ui " + projectArg', BUILD)
        self.assertIn('handler == "Preview"', IDE_CODE)

    def test_editor_can_search_active_file(self):
        self.assertIn('handler == "Find"', IDE_CODE)
        self.assertIn('onUI ? "uiText" : "codeText"', IDE_CODE)
        self.assertIn("Workspace.FindLine(data, term)", IDE_CODE)

    def test_ide_has_full_workspace_panels(self):
        for name in ("toolboxItems", "projectTitle", "projectFileList", "propertyName",
                     "compiler", "bottomTabs", "output", "status"):
            with self.subTest(id=name):
                self.assertIsNotNone(by_id(IDE, name))
        self.assertIn("SOLUTION EXPLORER", (ROOT / "Quark.hyui").read_text())

    def test_replace_buttons_and_unsaved_edit_handling(self):
        self.assertEqual(by_id(IDE, "replaceFirstButton").attrib["onClick"], "ReplaceFirst")
        self.assertEqual(by_id(IDE, "replaceAllButton").attrib["onClick"], "ReplaceAll")
        self.assertIsNotNone(by_id(IDE, "replaceQuery"))
        for name in ("ReplaceFirst", "ReplaceAll"):
            self.assertIn('handler == "' + name + '"', IDE_CODE)
        self.assertIn('Application.SetText(field, updated);', IDE_CODE)
        self.assertIn('save before building.', IDE_CODE)

    def test_replace_helper_handles_no_match_empty_and_nonoverlap(self):
        for function in ("FindAt", "CountMatches", "ReplaceFirst", "ReplaceAll"):
            self.assertIn("public static " + ("int" if function in ("FindAt", "CountMatches") else "string") + " " + function, WORKSPACE)
        self.assertIn('if (needle.Length == 0)', WORKSPACE)
        self.assertIn('at + needle.Length', WORKSPACE)
        native = (ROOT / "tests/WorkspaceSmoke.hy").read_text()
        self.assertIn('Workspace.ReplaceAll("aaa", "aa", "z")', native)
        self.assertIn('Workspace.ReplaceAll("text", "", "x")', native)

    def test_not_yet_supported_features_are_not_fake(self):
        self.assertIn("not implemented", README.lower())
        self.assertIn("not a fake in-editor picture", (ROOT / "Quark.hyui").read_text())
        self.assertNotIn('onClick="StartDebugging"', (ROOT / "Quark.hyui").read_text())

if __name__ == "__main__":
    unittest.main()
