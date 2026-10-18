import { Type } from "typebox";

//  What the model has while it builds a piece of the desktop with the user:
//  a creation's own files, pleamar's checks and documents, a look at what it
//  made and a try of it, and a look around the computer. Each tool only
//  PROPOSES: the build scene's logic decides —by fixed rules, asking the user
//  on a card when it has to—, does it, and answers. Every file goes through
//  it: it keeps the version before, checks what it asks the system for, and
//  answers with pleamar's verdict.
const file = Type.String({ pattern: "^[a-z0-9_-]+\\.(plm|luau|svg)$", maxLength: 40, description: "A file of the creation: scene.plm, scene.luau, another .plm or .luau beside them, icon.svg" });

export const TOOLS = [
    {
        name: "creation_read",
        looks: true,
        description: "Reads a file of the creation as it is now. Read before changing a file you did not write in this conversation: the user may have edited it by hand.",
        parameters: Type.Object({ file }),
    },
    {
        name: "creation_write",
        writes: true,
        description: "Writes a whole file of the creation. scene.plm is painted again at once, live, on the user's screen: write a first version early (the shape and its place), then improve it in steps. Marea keeps the version before, checks the permissions it asks for (the user is asked for anything new; dangerous commands are refused), and answers with `pleamar --check`'s verdict. While a version does not compile, the screen keeps the last good one: fix it with the error's line and column.",
        parameters: Type.Object({
            file,
            content: Type.String({ maxLength: 60000 }),
            why: Type.String({ maxLength: 160, description: "What this version changes, in a few words, in the user's language: it names the version in its history" }),
        }),
    },
    {
        name: "creation_check",
        looks: true,
        description: "Runs `pleamar --check` on the creation's scene: ok, or the first error with its line and column.",
        parameters: Type.Object({}),
    },
    {
        name: "creation_look",
        looks: true,
        description: "A picture of the creation as it is on the screen now, with a margin around it. Look after a change that matters to the eye, and compare it with what the user asked for.",
        parameters: Type.Object({}),
    },
    {
        name: "creation_name",
        looks: true,
        description: "Gives the creation its name: what the user sees at the top of the chat, and an app's name in the finder and the dock. Short, in the user's language («Notas», «Consumo IA»).",
        parameters: Type.Object({ name: Type.String({ maxLength: 40 }) }),
    },
    {
        name: "creation_key",
        description: "Gives the creation a key of its own in the window manager (pleamar-wm's keys.conf, read at once): «open» opens it (an app), «emit EVENT» or «fact NAME VALUE» tell its scene something (show or hide a panel). The user is asked first. Use it when they ask for a shortcut, or offer one for something they will open often.",
        parameters: Type.Object({
            keys: Type.String({ maxLength: 40, description: "Like Super+u or Super+Shift+n" }),
            order: Type.String({ maxLength: 100, description: "open · emit toggle · fact open true" }),
            why: Type.String({ maxLength: 160 }),
        }),
    },
    {
        name: "creation_poke",
        looks: true,
        description: "Uses the creation as the user would, to try it: «press ZONE» (a zone's name), «fact NAME VALUE», «emit EVENT», «get NAME». Answers with what changed (its facts and events). Look after it to see the result; then put it back as it was if the user should not find it changed.",
        parameters: Type.Object({ order: Type.String({ maxLength: 200, description: "press toggle · fact open true · emit refresh · get open" }) }),
    },
    {
        name: "system_run",
        looks: true,
        description: "Runs a program on the user's computer and answers with what it printed (its first 24 KB), to find things out as a developer would: where some data lives, what a program prints, what is installed, how another app gets what it shows. Reading is done at once; anything that can change the system is asked of the user first, and what is dangerous is refused. Not for the creation itself: it runs its own programs with run() in its logic, declared in permissions {}.",
        parameters: Type.Object({
            program: Type.String({ maxLength: 200, description: "The program alone, by its name (ls, find, grep, cat, node…) or its path; its arguments go in args. No shell: sh -c has to be approved by the user every time" }),
            args: Type.Optional(Type.Array(Type.String({ maxLength: 1000 }), { maxItems: 40 })),
            why: Type.String({ maxLength: 160, description: "What you want to find out, in the user's language: they read it if they are asked" }),
        }),
    },
    {
        name: "system_read",
        looks: true,
        description: "Reads a text file on the user's computer (its first 64 KB): a configuration, a script, another app's code, a data file. Files that may hold passwords or keys are asked of the user first.",
        parameters: Type.Object({
            path: Type.String({ maxLength: 500, description: "A whole path; ~ is the user's home" }),
            why: Type.String({ maxLength: 160 }),
        }),
    },
    {
        name: "pleamar_docs",
        looks: true,
        description: "pleamar's own documents. topic: reference (the whole language: every element, property, layer, gesture, service), guide, recipes (whole scenes that work), logic (the .luau), desktop (shells, apps, the window manager). With `find`, only the parts that mention it (a keyword, a property): the reference is long, ask it like that.",
        parameters: Type.Object({
            topic: Type.Union(["reference", "guide", "recipes", "logic", "desktop", "grammar"].map((t) => Type.Literal(t))),
            find: Type.Optional(Type.String({ maxLength: 60, description: "A word to find: `anchor`, `reserve`, `service audio`, `kind: window`" })),
        }),
    },
];

export function toolNamed(name) {
    return TOOLS.find((t) => t.name === name) ?? null;
}
