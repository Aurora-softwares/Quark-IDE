"""Quark Hydrogen/HYUI contract checks; native smoke testing remains separate."""
import pathlib
import re
import unittest
import xml.etree.ElementTree as ET

ROOT = pathlib.Path(__file__).resolve().parents[1]
UI = ET.parse(ROOT / "Quark.hyui").getroot()
PROGRAM = (ROOT / "src/Program.hy").read_text()
WORKSPACE = (ROOT / "src/Quark/Workspace.hy").read_text()
BUILD = (ROOT / "src/Quark/BuildService.hy").read_text()
MANIFEST = (ROOT / "Quark.hyproj").read_text()
README = (ROOT / "README.md").read_text()

def by_id(name):
    return UI.find(".//*[@id='%s']" % name)

class QuarkContracts(unittest.TestCase):
    def test_native_shell_has_workspace_regions(self):
        for name in ("screen", "editorTabs", "projectTitle", "codeText", "uiText", "output", "compiler"):
            with self.subTest(id=name):
                self.assertIsNotNone(by_id(name))

    def test_native_shell_actions_have_hydrogen_handlers(self):
        handlers = {node.attrib[key] for node in UI.iter()
                    for key in ("onClick", "onPick", "onCancel") if key in node.attrib}
        for name in handlers:
            with self.subTest(handler=name):
                self.assertIn('handler == "' + name + '"', PROGRAM)
        ids = [n.attrib["id"] for n in UI.iter() if "id" in n.attrib]
        self.assertEqual(len(ids), len(set(ids)))

    def test_project_manifest_has_only_hydrogen_sources(self):
        self.assertEqual(UI.tag, "Window")
        self.assertIn('ui = "Quark.hyui"', MANIFEST)
        self.assertNotIn(".rs", MANIFEST)
        self.assertIn("src/Hydrogen/UI/Application.hy", MANIFEST)

    def test_project_source_paths_allow_multiline_manifest(self):
        self.assertIn('text[cursor] != "]"', WORKSPACE)
        self.assertIn('cursor < text.Length', WORKSPACE)
        self.assertIn('QuotedSetting(manifest, "sources")', WORKSPACE)

    def test_project_picker_loads_real_manifest(self):
        self.assertEqual(by_id("startProjectPicker").attrib["onPick"], "ProjectPicked")
        self.assertEqual(by_id("workspaceProjectPicker").attrib["onPick"], "ProjectPicked")
        self.assertIn("System.IO.File.ReadAllText(path)", PROGRAM)

    def test_standalone_pickup_reads_both_file_types(self):
        self.assertEqual(by_id("startSourcePicker").attrib["onPick"], "SourcePicked")
        self.assertEqual(by_id("startUIPicker").attrib["onPick"], "UIPicked")
        self.assertIn("LoadDocument(args", PROGRAM)

    def test_save_is_connected_to_real_io(self):
        for name in ("SaveSource", "SaveUI", "SaveActive", "SaveAll"):
            self.assertIn('handler == "' + name + '"', PROGRAM)
        self.assertIn("System.IO.File.WriteAllText(path, content)", PROGRAM)

    def test_dirty_checks_both_buffers_before_switching(self):
        self.assertIn('Application.Value(args, "codeText")', PROGRAM)
        self.assertIn('Application.Value(args, "uiText")', PROGRAM)
        self.assertIn("if (Dirty(args))", PROGRAM)

    def test_project_creation_validates_filename(self):
        self.assertIn("public static bool ValidName", WORKSPACE)
        self.assertIn("IsIn(ch, \"0123456789\")", WORKSPACE)
        self.assertIn("if (i == 0 && number)", WORKSPACE)

    def test_project_creation_does_not_overwrite_starter_files(self):
        self.assertIn("System.IO.File.Exists(project)", WORKSPACE)
        self.assertIn("System.IO.File.Exists(source)", WORKSPACE)
        self.assertIn("System.IO.File.Exists(layout)", WORKSPACE)
        self.assertIn("System.IO.File.Exists(bridge)", WORKSPACE)

    def test_starter_hyui_has_connected_click_event(self):
        self.assertIn('onClick=\\\"Hello\\\"', WORKSPACE)
        self.assertIn('Application.Handler(args) ==', WORKSPACE)
        self.assertIn('Application.SetText(', WORKSPACE)

    def test_starter_creation_generates_manifest_and_backend(self):
        self.assertIn('sources = [\\\"Program.hy\\\", \\\"Hydrogen.UI.hy\\\"]', WORKSPACE)
        self.assertIn('ui = \\\"Main.hyui\\\"', WORKSPACE)
        self.assertIn('System.IO.File.WriteAllText(project, manifest)', WORKSPACE)

    def test_compiler_checks_use_real_subprocess(self):
        self.assertIn('command + " check " + projectArg', BUILD)
        self.assertIn("System.Host.Process.Run(", BUILD)
        self.assertIn("Exit code:", BUILD)

    def test_build_persists_stdout_and_stderr(self):
        self.assertIn('command + " build " + projectArg + " -o " + outputArg', BUILD)
        self.assertIn('log + " 2>&1"', BUILD)
        self.assertIn("System.IO.File.ReadAllText(logPath)", BUILD)

    def test_failed_build_cannot_launch_program(self):
        self.assertIn("if (built.Code() != 0)", BUILD)
        self.assertIn("if (!System.IO.File.Exists(executable))", BUILD)

    def test_ui_preview_uses_official_hy_command(self):
        self.assertIn('command + " ui " + projectArg', BUILD)
        self.assertIn("if (!hasUI)", BUILD)
        self.assertIn('handler == "Preview"', PROGRAM)

    def test_search_scans_buffer_lines(self):
        self.assertIn('if (body[i] == "\\n")', WORKSPACE)
        self.assertIn("return line;", WORKSPACE)

    def test_search_uses_active_document(self):
        self.assertIn('handler == "Find"', PROGRAM)
        self.assertIn('onUI ? "uiText" : "codeText"', PROGRAM)
        self.assertIn('Application.SetText("searchResult"', PROGRAM)

    def test_no_debugger_or_designer_parity_claim(self):
        self.assertIn("not yet implemented", README.lower())
        self.assertIn("not a fake in-editor picture", (by_id("uiTab").find(".//Label[@text]") or UI).attrib.get("text", "") + (ROOT / "Quark.hyui").read_text())
        self.assertNotIn('onClick="StartDebugging"', (ROOT / "Quark.hyui").read_text())

if __name__ == "__main__":
    unittest.main()
