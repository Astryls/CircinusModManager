#!/usr/bin/env node
// What a command line survives on the way to the program.
//
//   node tools/loadtest/release-args.mjs
//
// v1.0.0 failed to tag because `git commit -m Circinus Mod Manager 1.0.0` went through a Windows
// shell that split it back into words, and git read three of them as pathspecs. The bug was not
// in git and not in the message: it was that a list of arguments was handed to something that
// only takes a string. So this asserts both halves of the rule -- that a shell is used only for
// the commands that cannot run without one, and that when one is used the arguments come out the
// far side unchanged -- and it asserts the second half by actually running a program through a
// shell and reading back what it received.
import { spawnSync } from "node:child_process";
import { writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { quote, forShell, newer } from "../release.mjs";

let bad = 0;
const ok = (what, got, want) => {
  const same = JSON.stringify(got) === JSON.stringify(want);
  if (!same) bad++;
  console.log(`  ${same ? "ok  " : "FAIL"}  ${what}${same ? "" : `\n          got  ${JSON.stringify(got)}\n          want ${JSON.stringify(want)}`}`);
};

console.log("\nWhich commands get a shell");
// git, cargo and node are real executables and need no shell; npm and gh are .cmd files on
// Windows, which Node will not start on its own. Giving a shell to something that does not need
// one is exactly how the arguments got re-split.
for (const cmd of ["git", "cargo", "node"]) {
  ok(`${cmd} keeps its arguments as a list`, forShell(cmd, ["-m", "Circinus Mod Manager 1.0.0"]), ["-m", "Circinus Mod Manager 1.0.0"]);
}
for (const cmd of ["npm", "gh"]) {
  const quoted = forShell(cmd, ["-m", "Circinus Mod Manager 1.0.0"]);
  const wanted = process.platform === "win32" ? ["-m", '"Circinus Mod Manager 1.0.0"'] : ["-m", "Circinus Mod Manager 1.0.0"];
  ok(`${cmd} is quoted for its shell (on Windows)`, quoted, wanted);
}

console.log("\nWhat quoting protects");
ok("a plain word is left alone", quote("--force"), "--force");
ok("a version is left alone", quote("1.0.0"), "1.0.0");
ok("spaces are wrapped", quote("Circinus Mod Manager 1.0.0"), '"Circinus Mod Manager 1.0.0"');
ok("an empty argument survives as an empty argument", quote(""), "");
ok("quotes inside are escaped, not dropped", quote('say "hi" there'), '"say \\"hi\\" there"');
ok("an ampersand cannot start a second command", quote("notes & rm -rf /"), '"notes & rm -rf /"');
ok("a pipe cannot redirect", quote("a | b"), '"a | b"');

console.log("\nThrough a real shell, and back");
// The failure is not specific to cmd.exe: any shell splits a string on spaces. Running a program
// through this machine's shell and asking it what it received proves the quoting holds, and
// proves it the only way that counts -- by the arguments the program actually got.
// A program that does nothing but say what it was given. It has to be a file rather than `node
// -p`, because an argument like `-m` in front of a script is read by node as an option to node.
const echo = path.join(tmpdir(), `circinus-argv-${process.pid}.cjs`);
writeFileSync(echo, "process.stdout.write(JSON.stringify(process.argv.slice(2)));\n");
const argv = (args, useShell) => {
  const line = useShell ? [quote(echo), ...args.map(quote)] : [echo, ...args];
  const r = spawnSync(process.execPath, line, { encoding: "utf8", shell: useShell });
  try {
    return JSON.parse(r.stdout.trim());
  } catch {
    return { failed: `${r.stdout}${r.stderr}`.trim() };
  }
};
const notes = ["-m", "Circinus Mod Manager 1.0.0"];
ok("no shell: the message stays one argument", argv(notes, false), notes);
ok("shell, quoted: the message stays one argument", argv(notes, true), notes);
// And the bug itself, so that if the quoting is ever removed this test says what breaks rather
// than quietly passing: unquoted through a shell, one argument becomes four.
const naked = spawnSync(process.execPath, [echo, ...notes], { encoding: "utf8", shell: true });
ok("shell, unquoted: this is the bug, four arguments", JSON.parse(naked.stdout.trim()), ["-m", "Circinus", "Mod", "Manager", "1.0.0"]);
rmSync(echo, { force: true });

console.log("\nVersions");
ok("1.0.0 is newer than 0.9.9", newer("1.0.0", "0.9.9"), true);
ok("1.0.0 is newer than its own pre-release", newer("1.0.0", "1.0.0-beta.1"), true);
ok("0.9.9 is not newer than 1.0.0", newer("0.9.9", "1.0.0"), false);
ok("a version is not newer than itself", newer("1.0.0", "1.0.0"), false);

console.log(bad ? `\n${bad} failed.\n` : "\nAll good.\n");
process.exit(bad ? 1 : 0);
