using System;
using Quark;

// A real Hydrogen executable test of the shared Quark workspace backend.
public class Program {
    public static int Main(string[] args) {
        if (Workspace.Directory("/tmp/Quark/Program.hy") != "/tmp/Quark") { return 1; }
        if (Workspace.BaseName("/tmp/Quark/Program.hy") != "Program.hy") { return 2; }
        if (Workspace.Join("/tmp/Quark", "Program.hy") != "/tmp/Quark/Program.hy") { return 3; }
        if (!Workspace.ValidName("Project_24") || Workspace.ValidName("24Project") ||
            Workspace.ValidName("../escape") || Workspace.ValidName("")) { return 4; }
        if (!Workspace.SafeRelative("src/Program.hy") || Workspace.SafeRelative("../other.hy") ||
            Workspace.SafeRelative("/etc/passwd")) { return 5; }
        string project = "format = 2\nname = \"Example\"\nsources = [\n  \"src/Main.hy\",\n  \"src/Extra.hy\"\n]\nui = \"Main.hyui\"\n";
        if (Workspace.QuotedSetting(project, "name") != "Example") { return 6; }
        if (Workspace.QuotedSetting(project, "sources") != "src/Main.hy") { return 7; }
        if (Workspace.SourceAt("/repo/Example.hyproj", project, 0) != "/repo/src/Main.hy") { return 8; }
        if (Workspace.SourceAt("/repo/Example.hyproj", project, 1) != "/repo/src/Extra.hy") { return 9; }
        if (Workspace.SourceAt("/repo/Example.hyproj", project, 2) != "") { return 10; }
        if (Workspace.SourceCount("/repo/Example.hyproj", project) != 2) { return 11; }
        if (Workspace.Layout("/repo/Example.hyproj", project) != "/repo/Main.hyui") { return 12; }
        if (Workspace.ExplorerItems("/repo/Example.hyproj", project) != "1. Main.hy\n2. Extra.hy\n3. Main.hyui\n4. Example.hyproj") { return 13; }
        if (Workspace.FindLine("one\ntwo\nthree", "three") != 3 ||
            Workspace.FindLine("one\ntwo", "missing") != 0) { return 14; }
        if (Workspace.Preview("short") != "short") { return 15; }
        if (Workspace.FindAt("aabba", "bb", 0) != 2 ||
            Workspace.FindAt("aabba", "bb", 3) != -1) { return 16; }
        if (Workspace.CountMatches("abababa", "aba") != 2 ||
            Workspace.CountMatches("abc", "") != 0) { return 17; }
        if (Workspace.ReplaceFirst("alpha beta alpha", "alpha", "next") !=
            "next beta alpha") { return 18; }
        if (Workspace.ReplaceAll("alpha beta alpha", "alpha", "next") !=
            "next beta next") { return 19; }
        if (Workspace.ReplaceAll("aaa", "aa", "z") != "za" ||
            Workspace.ReplaceAll("text", "", "x") != "text" ||
            Workspace.ReplaceFirst("abc", "missing", "z") != "abc") { return 20; }
        if (Workspace.ReplaceAll("one\ntwo\none", "one", "") != "\ntwo\n" ||
            Workspace.ReplaceAll("hi", "hi", "hi hi") != "hi hi") { return 21; }
        System.Console.WriteLine("Quark Hydrogen workspace tests passed");
        return 0;
    }
}
