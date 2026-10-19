#!/usr/bin/env python3
"""Writes the shore —the dock of a free monitor— into examples/session.plm.

The scene language has no functions and a monitor's dock cannot be a
`repeat` (each one is named by its number), so the section is written out
here: four monitors, twelve programs, eight pools. Run it from the repo:

    tools/gen-shore.py            # replaces the section in examples/session.plm

The section goes from the line that opens it («// ── the shore: …») to the
one before «// ── the pools at a glance». What reads it from elsewhere in the
scene (a window put away, its preview, its row in a menu) is written by hand
there, with the names this keeps: dk.q.N, dk.a0.N, dk.K.N, pv.bx.N, mn.x.N…
"""
import re
import sys
from pathlib import Path

MONITORS = 4
ITEMS = 12
POOLS = 8
ROWS = 6  # windows listed in an item's menu

SCENE = Path(__file__).resolve().parent.parent / "examples" / "session.plm"
START = "    // ── the shore: the dock of a free monitor."
END = "    // ── the pools at a glance (Super+Tab)"


def left(d):
    # What the icons before a point have grown, as the bump under the mouse
    # is shared out: the integral of (1 - |d|)² up to it, 0 to 2/3.
    return f"if({d} < -1, 0, if({d} < 0, pow(1 + {d}, 3) / 3, if({d} < 1, (2 - pow(1 - {d}, 3)) / 3, 2 / 3)))"


def gen():
    o = []
    w = o.append
    w("""    // ── the shore: the dock of a free monitor. When a monitor goes free, the
    // water rises at its edge and leaves the shore there, joined to it by a
    // neck, as Marea hangs from hers at the top: the programs pinned (`dock …`
    // in session.conf) and the others with windows on the pool shown, each
    // with a drop per window under it (mint, the one with the keyboard); the
    // windows put away, floating as they are; and the pools of the monitor,
    // last. Hovering, the icons rise like buoys on a passing wave —or swell,
    // the nearest most, with magnification—; pressed, one dips and rings the
    // water, and while its program starts it hops. Tiled, there is no shore,
    // and nothing is put away.
    // How it behaves is the control centre's (`dock-style …` in
    // session.conf, given here at the start as facts): hidden until the mouse
    // is pushed against its edge, magnified, with or without the drops, the
    // windows put away into their program's icon, its size, and which edge
    // —bottom, left or right—. Written by tools/gen-shore.py: change it
    // there. ──
    spring rise = 190, 20
    fact dock.hide = 0
    fact dock.mag = 0
    fact dock.dots = 1
    fact dock.into = 0
    fact dock.size = 44
    fact dock.side = 0                   // 0 bottom · 1 left · 2 right
    // An icon, the room it has along the shore, the shore across, the room
    // of a window put away; and how much the one under the mouse swells.
    let dkic = clamp(dock.size, 32, 72)
    let dkf = dkic / 44
    let dkcell = dkic + 16
    let dkpl = dkic + 20
    let dkmc = dkic + 32
    let dkvert = dock.side > 0.5
    let dklift = if(dock.mag > 0.5, 0.75, 0)
    let dkreach = dkcell * 2.6
""")
    for s in range(MONITORS):
        W = f"screen.{s}.width"
        H = f"screen.{s}.height"

        def X(a, q):
            return f"if(dkvert, dk.cr.{s} + dk.cs.{s} * ({q}), {a})"

        def Y(a, q):
            return f"if(dkvert, {a}, dk.cr.{s} + dk.cs.{s} * ({q}))"

        def shifted(name, c0):
            # A thing along the shore at `c0` with nothing swelling, moved
            # aside as the icons before it grow.
            w(f"    let {name}.c.{s} = {c0}")
            w(f"    let {name}.d.{s} = ({name}.c.{s} - dk.ma.{s}) / dkreach")
            w(f"    let {name}.{s} = {name}.c.{s} + dk.K.{s} * ({left(f'{name}.d.{s}')} - 1 / 3)")

        w(f"""    // ── monitor {s}'s shore ──
    // Shown while the monitor is free, unless it hides: then while the
    // mouse pushes against its edge, or its menu, previews or a carried icon
    // are out. What it keeps clear for the windows, only when it does not hide.
    prop shore.{s} = 0 ~rise
    follow shore.{s} = if(free.{s} > 0.5 and (dock.hide < 0.5 or dk.open.{s} or dmenu.{s} >= 0 or pv.{s} >= 0 or dkdrag.{s} >= 0), 1, 0)
    prop room.{s} = 0 ~rise
    follow room.{s} = if(free.{s} > 0.5 and dock.hide < 0.5, 1, 0)
    fact dk.open.{s} = false
    // The item being carried out of it, the one whose windows rise from the
    // water, the one whose menu is open (and the one it was for).
    fact dkdrag.{s} = -1
    fact pv.{s} = -1
    fact dmenu.{s} = -1
    fact dmk.{s} = 0
    fact poofx.{s} = 0
    fact poofy.{s} = 0
    prop poof.{s} = 1 ~700ms
    prop pvup.{s} = 0 ~pop
    follow pvup.{s} = if(pv.{s} >= 0, 1, 0)
    prop mnup.{s} = 0 ~pop
    follow mnup.{s} = if(dmenu.{s} >= 0, 1, 0)
    let dk.n.{s} = win.docks.{s}
    let dk.m.{s} = if(dock.into > 0.5, 0, win.mins.{s})
    let dk.p.{s} = max(win.pools.{s}, win.used.{s} + 1)
    let dk.tw.{s} = 28 + dk.n.{s} * dkcell + if(dk.m.{s} > 0, 20 + dk.m.{s} * dkmc, 0) + 20 + dk.p.{s} * 26
    prop dkw.{s} = 200 ~pop
    follow dkw.{s} = dk.tw.{s}
    // Across: a point `q` out from its edge is at dk.cr + dk.cs · q, in x on
    // a side and in y at the bottom. Along: from dk.a0, centred.
    let dk.cr.{s} = if(dock.side == 1, 0, if(dock.side == 2, {W}, {H}))
    let dk.cs.{s} = if(dock.side == 1, 1, -1)
    let dk.a0.{s} = if(dkvert, {H}, {W}) / 2 - dkw.{s} / 2
    let dk.q.{s} = 0 - (dkpl / 2 + 10) + (dkpl + 24) * shore.{s}
    // Where the mouse is along it and how far from its edge.
    let dk.ma.{s} = if(dkvert, mouse.y, mouse.x)
    let dk.mq.{s} = if(dock.side == 1, mouse.x, if(dock.side == 2, {W} - mouse.x, {H} - mouse.y))
    // Magnified, how much the icons before a point have moved aside, and
    // how much longer the whole shore is.
    prop dk.near.{s} = 0 ~180ms
    let dk.K.{s} = dk.near.{s} * dklift * dkic * dkreach / dkcell
    let dk.grow.{s} = dk.K.{s} * 2 / 3
    let dk.on.{s} = mon == {s} and shore.{s} > 0.6 and dk.mq.{s} <= dk.q.{s} + dkpl / 2 + 6 and dk.ma.{s} >= dk.a0.{s} - dk.grow.{s} / 2 - 10 and dk.ma.{s} <= dk.a0.{s} + dkw.{s} + dk.grow.{s} / 2 + 10
    follow dk.near.{s} = if(dk.on.{s} and dkdrag.{s} < 0, 1, 0)
    let dk.t.{s} = (dk.ma.{s} - dk.a0.{s} - 14) / dkcell - 0.5
    let dk.hk.{s} = clamp(floor(dk.t.{s} + 0.5), 0, {ITEMS - 1})
    // Hidden, it comes out while the mouse is pushed against its edge, and
    // goes once the mouse has left it (not while one of its menus is out).
    on change mouse.x + mouse.y * 8192 while dock.hide > 0.5 and not dk.open.{s} and free.{s} > 0.5 and mon == {s} and dk.mq.{s} <= 2 and dragging < 0 {{ dk.open.{s} = true }}
    on change mouse.x + mouse.y * 8192 while dk.open.{s} and (mon != {s} or dk.mq.{s} > dk.q.{s} + dkpl / 2 + 60) and dmenu.{s} < 0 and pv.{s} < 0 and dkdrag.{s} < 0 {{ dk.open.{s} = false }}
    on change dock.hide while dock.hide < 0.5 {{ dk.open.{s} = false }}""")
        for k in range(ITEMS):
            w(f"""    image dockicon.{s}.{k} = from win.dock.{s}.{k}.icon, 96, 96
    prop buoy.{s}.{k} = 0 ~pop
    follow buoy.{s}.{k} = if(dk.on.{s} and dock.mag < 0.5, pow(max(0, 1 - abs({k} - dk.t.{s}) / 1.8), 2), 0)
    prop here.{s}.{k} = 0 ~pop
    follow here.{s}.{k} = if({k} < dk.n.{s}, 1, 0)
    prop ring.{s}.{k} = 1 ~760ms
    prop hop.{s}.{k} = 0 ~560ms
    follow hop.{s}.{k} = 0
    fact hops.{s}.{k} = 0""")
            shifted(f"dka{k}", f"dk.a0.{s} + 14 + {k} * dkcell + dkcell / 2")
            w(f"    let dksz{k}.{s} = dkic * (1 + dklift * dk.near.{s} * pow(max(0, 1 - abs(dka{k}.d.{s})), 2)) * (1 + 0.14 * buoy.{s}.{k})")
            w(f"    let dkiq{k}.{s} = dk.q.{s} + 3 + 12 * buoy.{s}.{k} + (dksz{k}.{s} - dkic) * 0.55 + 104 * dkf * hop.{s}.{k} * (1 - hop.{s}.{k})")
        shifted("dks1", f"dk.a0.{s} + 14 + dk.n.{s} * dkcell + 10")
        shifted("dks2", f"dk.a0.{s} + dkw.{s} - 14 - dk.p.{s} * 26 - 10")
        for j in range(POOLS):
            shifted(f"dkp{j}", f"dk.a0.{s} + dkw.{s} - 14 - dk.p.{s} * 26 + {j} * 26 + 13")

        # ── the water and what floats on it ──
        L = f"(dkw.{s} + dk.grow.{s}) * (0.55 + 0.45 * shore.{s})"
        mid = f"dk.a0.{s} + dkw.{s} / 2"
        w(f"""    group {{
        z: 99982
        show: shore.{s} > 0.01 and screen.index == {s}
        body {{
            gradient: {X(0, f'dk.q.{s} + dkpl / 2')}, {Y(0, f'dk.q.{s} + dkpl / 2')} to {X(0, f'dk.q.{s} - dkpl / 2')}, {Y(0, f'dk.q.{s} - dkpl / 2')}, #15181b, #0c0e10 45%, #0a0b0d
            rim: 12%
            shadow: 0, 12, 34, 42%
            box {{ from: if(dkvert, if(dock.side == 1, -140, {W}), -80), if(dkvert, -80, {H}); size: if(dkvert, 140, {W} + 160), if(dkvert, {H} + 160, 140) }}
            box {{ at: {X(mid, f'dk.q.{s}')}, {Y(mid, f'dk.q.{s}')}; size: if(dkvert, dkpl, {L}), if(dkvert, {L}, dkpl); corner: 22 * dkf; blend: 22 + 36 * 4 * shore.{s} * (1 - shore.{s}) }}
        }}""")
        for k in range(ITEMS):
            a = f"dka{k}.{s}"
            sz = f"dksz{k}.{s}"
            iq = f"dkiq{k}.{s}"
            dq = f"dk.q.{s} - 25 * dkf"
            dot = f"radius: if(win.dock.{s}.{k}.focused, 2.4, 2); color: if(win.dock.{s}.{k}.focused, mint, ink); opacity: if(win.dock.{s}.{k}.focused, 95%, 55%)"
            dots = f"dock.dots > 0.5 and "
            w(f"""        group {{
            show: here.{s}.{k} > 0.01
            opacity: here.{s}.{k}
            ellipse {{ at: {X(a, f'dk.q.{s} - 22 * dkf')}, {Y(a, f'dk.q.{s} - 22 * dkf')}; radius: 18 + 30 * ring.{s}.{k}; scale: if(dkvert, 0.32, 1), if(dkvert, 1, 0.32); stroke: 1.4 * (1 - ring.{s}.{k}) + 0.3; color: ink; opacity: 45% * (1 - ring.{s}.{k}); show: ring.{s}.{k} < 0.99 }}
            image dockicon.{s}.{k} {{ at: {X(a, iq)} - {sz} / 2 * here.{s}.{k}, {Y(a, iq)} - {sz} / 2 * here.{s}.{k}; size: {sz} * here.{s}.{k}, {sz} * here.{s}.{k} }}
            ellipse {{ at: {X(a, dq)}, {Y(a, dq)}; {dot}; show: {dots}(win.dock.{s}.{k}.windows == 1 or win.dock.{s}.{k}.windows >= 3) }}
            ellipse {{ at: {X(f'{a} - 3.5', dq)}, {Y(f'{a} - 3.5', dq)}; {dot}; show: {dots}win.dock.{s}.{k}.windows == 2 }}
            ellipse {{ at: {X(f'{a} + 3.5', dq)}, {Y(f'{a} + 3.5', dq)}; {dot}; show: {dots}win.dock.{s}.{k}.windows == 2 }}
            ellipse {{ at: {X(f'{a} - 6', dq)}, {Y(f'{a} - 6', dq)}; {dot}; show: {dots}win.dock.{s}.{k}.windows >= 3 }}
            ellipse {{ at: {X(f'{a} + 6', dq)}, {Y(f'{a} + 6', dq)}; {dot}; show: {dots}win.dock.{s}.{k}.windows >= 3 }}
            zone box dockz.{k}.{s} {{ at: {X(a, f'dk.q.{s}')}, {Y(a, f'dk.q.{s}')}; size: if(dkvert, dkpl, dkcell), if(dkvert, dkcell, dkpl); cursor: pointer; active: shore.{s} > 0.6 and {k} < dk.n.{s} and not overview; label: "{{win.dock.{s}.{k}.name}}" }}
        }}""")
        w(f"""        // Between the programs and what is put away, and before the pools: a line of water.
        box {{ at: {X(f'dks1.{s}', f'dk.q.{s}')}, {Y(f'dks1.{s}', f'dk.q.{s}')}; size: if(dkvert, 30, 1), if(dkvert, 1, 30); color: ink; opacity: 18%; show: dk.m.{s} > 0 }}
        box {{ at: {X(f'dks2.{s}', f'dk.q.{s}')}, {Y(f'dks2.{s}', f'dk.q.{s}')}; size: if(dkvert, 30, 1), if(dkvert, 1, 30); color: ink; opacity: 18% }}""")
        for j in range(POOLS):
            p = f"dkp{j}.{s}"
            at = f"{X(p, f'dk.q.{s}')}, {Y(p, f'dk.q.{s}')}"
            w(f"""        ellipse {{ at: {at}; radius: 8; stroke: 1.3; color: ink; opacity: if({j + 1} == dk.p.{s} and win.used.{s} < {j + 1}, 22%, 45%); show: {j} < dk.p.{s} and win.shown.{s} != {j + 1} }}
        ellipse {{ at: {at}; radius: 8; color: mint; opacity: 90%; show: {j} < dk.p.{s} and win.shown.{s} == {j + 1} }}
        text "{j + 1}" {{ at: {at}; anchor: center; size: 9; weight: 700; color: #0c0e10; show: {j} < dk.p.{s} and win.shown.{s} == {j + 1} }}
        zone ellipse dkpool.{j}.{s} {{ at: {at}; radius: 12; cursor: pointer; active: shore.{s} > 0.6 and {j} < dk.p.{s} and not overview; label: "Workspace {j + 1}" }}""")
        w("    }")

        # ── what pressing them does ──
        far = f"dk.mq.{s} > dk.q.{s} + dkpl / 2 + 78"
        w("""    // Dragged, its icon goes out of the shore (far enough, unpinned, it
    // bursts into drops); only past a few pixels: a real mouse moves one
    // between press and release, and the click that opens the program was
    // taken for a drag. Pressed with nothing open, it hops while its program
    // starts (three times at most, until a window comes).""")
        for k in range(ITEMS):
            w(f"""    on press dockz.{k}.{s} {{ impulse buoy.{s}.{k} -9 }}
    on drag dockz.{k}.{s} while dkdrag.{s} < 0 and (abs(drag.dx) > 6 or abs(drag.dy) > 6) {{
        dkdrag.{s} = {k}
        pv.{s} = -1
    }}
    on release dockz.{k}.{s} while dkdrag.{s} < 0 {{
        dock {k} on {s}
        pv.{s} = -1
        ring.{s}.{k}: 0 ~0ms
        ring.{s}.{k}: 1 ~760ms after 16ms
    }}
    on release dockz.{k}.{s} while dkdrag.{s} < 0 and win.dock.{s}.{k}.windows == 0 {{
        hops.{s}.{k} = 3
        hop.{s}.{k}: 1 ~0ms
    }}
    on change hop.{s}.{k} while hop.{s}.{k} < 0.02 and hops.{s}.{k} > 1 and win.dock.{s}.{k}.windows == 0 {{
        hops.{s}.{k} = hops.{s}.{k} - 1
        hop.{s}.{k}: 1 ~0ms
    }}
    on release dockz.{k}.{s} while dkdrag.{s} == {k} and {far} and win.dock.{s}.{k}.pinned > 0.5 {{
        dock {k} on {s} unpin
        poofx.{s} = mouse.x
        poofy.{s} = mouse.y
        poof.{s}: 0 ~0ms
        poof.{s}: 1 ~700ms after 16ms
    }}
    on release dockz.{k}.{s} {{ dkdrag.{s} = -1 }}
    on drop dockz.{k}.{s} {{
        dock {k} on {s} with drop
        ring.{s}.{k}: 0 ~0ms
        ring.{s}.{k}: 1 ~760ms after 16ms
    }}
    on press right dockz.{k}.{s} {{
        dmenu.{s} = {k}
        dmk.{s} = {k}
        pv.{s} = -1
    }}
    on hover dockz.{k}.{s} for 450ms while dkdrag.{s} < 0 and dmenu.{s} < 0 and win.dock.{s}.{k}.windows > 0 {{ pv.{s} = {k} }}""")
        for j in range(POOLS):
            w(f"    on press dkpool.{j}.{s} {{ workspace {j + 1} on {s} }}")

        # ── previews and the menu: where they go ──
        items = lambda f: ", ".join(f(k) for k in range(ITEMS))
        w(f"""    // Its previews, as many as it has windows: over its icon at the bottom,
    // beside it on a side (window.plm's per window: pv.bx + pv.sx · turn).
    let pv.n.{s} = pick(max(pv.{s}, 0), {items(lambda k: f'win.dock.{s}.{k}.windows')})
    let pv.ia.{s} = pick(max(pv.{s}, 0), {items(lambda k: f'dka{k}.{s}')})
    let pv.bx.{s} = if(dkvert, dk.cr.{s} + dk.cs.{s} * (dk.q.{s} + dkpl / 2 + 127), clamp(pv.ia.{s}, pv.n.{s} * 116 + 20, {W} - pv.n.{s} * 116 - 20) - (pv.n.{s} - 1) * 116)
    let pv.sx.{s} = if(dkvert, dk.cs.{s} * 232, 232)
    let pv.by.{s} = if(dkvert, clamp(pv.ia.{s}, 110, {H} - 110) - 10, {H} - dk.q.{s} - 140)
    let pv.ox.{s} = if(dkvert, 0 - dk.cs.{s} * 30, 0)
    let pv.oy.{s} = if(dkvert, 0, 30)
    let pv.ext.{s} = dk.q.{s} + dkpl / 2 + 16 + pv.n.{s} * 232 + 24
    let pv.x1.{s} = if(dkvert, if(dock.side == 1, 0, {W} - pv.ext.{s}), pv.bx.{s} - 135)
    let pv.x2.{s} = if(dkvert, if(dock.side == 1, pv.ext.{s}, {W}), pv.bx.{s} + (pv.n.{s} - 1) * 232 + 135)
    let pv.y1.{s} = pv.by.{s} - 99
    let pv.y2.{s} = if(dkvert, pv.by.{s} + 119, {H})
    // Its menu: the item's windows (window.plm's rows), then New window and
    // Hide while it has some, keep it on the shore or take it off, and close
    // them —or Open, with none—. Over its icon at the bottom, beside it on a side.
    let mn.wn.{s} = min(pick(max(dmenu.{s}, 0), {items(lambda k: f'win.dock.{s}.{k}.windows')}), {ROWS})
    let mn.run.{s} = pick(dmk.{s}, {items(lambda k: f'win.dock.{s}.{k}.windows')}) > 0
    let mn.pin.{s} = pick(dmk.{s}, {items(lambda k: f'win.dock.{s}.{k}.pinned')}) > 0.5
    let mn.base.{s} = 6 + mn.wn.{s} * 28 + if(mn.wn.{s} > 0, 10, 0)
    let mn.h.{s} = mn.base.{s} + if(mn.run.{s}, 4, 2) * 28 + 6
    let mn.ia.{s} = pick(dmk.{s}, {items(lambda k: f'dka{k}.{s}')})
    let mn.x.{s} = if(dkvert, if(dock.side == 1, dk.q.{s} + dkpl / 2 + 14, {W} - dk.q.{s} - dkpl / 2 - 14 - 220), clamp(mn.ia.{s} - 110, 8, {W} - 228))
    let mn.y.{s} = if(dkvert, clamp(mn.ia.{s} - mn.h.{s} / 2, 8, {H} - mn.h.{s} - 8), {H} - dk.q.{s} - dkpl / 2 - 14 - mn.h.{s})
    // Away from the shore, its previews and its menu, they sink.
    let pv.near.{s} = mon == {s} and (dk.on.{s} or (pv.{s} >= 0 and mouse.x >= pv.x1.{s} and mouse.x <= pv.x2.{s} and mouse.y >= pv.y1.{s} and mouse.y <= pv.y2.{s}) or (dmenu.{s} >= 0 and mouse.x >= mn.x.{s} - 24 and mouse.x <= mn.x.{s} + 244 and mouse.y >= mn.y.{s} - 24 and mouse.y <= mn.y.{s} + mn.h.{s} + 24))
    on change pv.near.{s} while not pv.near.{s} {{
        pv.{s} = -1
        dmenu.{s} = -1
    }}
    on key Escape while dmenu.{s} >= 0 or pv.{s} >= 0 {{
        dmenu.{s} = -1
        pv.{s} = -1
    }}""")

        # ── over the water: names, what is unread, the one carried, the menu ──
        w(f"""    group {{
        z: 99984
        show: shore.{s} > 0.01 and screen.index == {s}""")
        for k in range(ITEMS):
            a = f"dka{k}.{s}"
            sz = f"dksz{k}.{s}"
            iq = f"dkiq{k}.{s}"
            tw = f"(tip.{s}.{k}.width + 22)"
            tx = f"if(dkvert, dk.cr.{s} + dk.cs.{s} * ({iq} + {sz} / 2 + 14 + {tw} / 2), {a})"
            ty = f"if(dkvert, {a}, dk.cr.{s} + dk.cs.{s} * ({iq} + {sz} / 2 + 41))"
            ix = X(a, iq)
            iy = Y(a, iq)
            w(f"""        measure tip.{s}.{k}
        group {{
            show: dk.on.{s} and dk.hk.{s} == {k} and {k} < dk.n.{s} and pv.{s} < 0 and dmenu.{s} < 0 and dkdrag.{s} < 0
            body {{
                color: #0c0e10
                rim: 12%
                shadow: 0, 6, 16, 40%
                box {{ at: {tx}, {ty}; size: {tw}, 26; corner: 13 }}
            }}
            text win.dock.{s}.{k}.name {{ at: {tx}, {ty}; anchor: center; size: 12; weight: 600; color: ink; measure: tip.{s}.{k}; lines: 1 }}
        }}
        // Unread, a drop of coral on its corner.
        group {{
            show: win.dock.{s}.{k}.badge > 0 and {k} < dk.n.{s}
            body {{
                color: #ef7a66
                rim: 20%
                shadow: 0, 2, 5, 40%
                ellipse {{ at: {ix} + 0.39 * {sz}, {iy} - 0.39 * {sz}; radius: 8.5 }}
            }}
            text "{{win.dock.{s}.{k}.badge, 0}}" {{ at: {ix} + 0.39 * {sz}, {iy} - 0.39 * {sz}; anchor: center; size: 10; weight: 700; color: #1a0c09 }}
        }}""")
        w(f"""        // Carried out of the shore: the icon at the mouse, and far enough, it
        // says it will go.
        group {{
            show: dkdrag.{s} >= 0 and mon == {s}
            opacity: if({far}, 55%, 90%)""")
        for k in range(ITEMS):
            w(f"            image dockicon.{s}.{k} {{ at: mouse.x - 24, mouse.y - 24; size: 48, 48; show: dkdrag.{s} == {k} }}")
        w(f"""        }}
        // Unpinned, it bursts into drops where it was let go.
        group {{
            show: poof.{s} < 0.99
            opacity: 1 - poof.{s}
            ellipse {{ at: poofx.{s}, poofy.{s}; radius: 10 + 34 * poof.{s}; stroke: 2 * (1 - poof.{s}) + 0.4; color: ink; opacity: 60% }}
            ellipse {{ at: poofx.{s} - 22 * poof.{s}, poofy.{s} - 16 * poof.{s} + 30 * poof.{s} * poof.{s}; radius: 3.5 * (1 - poof.{s}); color: ink }}
            ellipse {{ at: poofx.{s} + 20 * poof.{s}, poofy.{s} - 20 * poof.{s} + 34 * poof.{s} * poof.{s}; radius: 3 * (1 - poof.{s}); color: ink }}
            ellipse {{ at: poofx.{s} + 4 * poof.{s}, poofy.{s} - 28 * poof.{s} + 40 * poof.{s} * poof.{s}; radius: 2.5 * (1 - poof.{s}); color: ink }}
        }}
        // Its menu.
        group {{
            show: mnup.{s} > 0.01
            opacity: mnup.{s}
            pivot: {X(f'mn.ia.{s}', f'dk.q.{s}')}, {Y(f'mn.ia.{s}', f'dk.q.{s}')}
            scale: 0.9 + 0.1 * mnup.{s}
            body {{
                color: #0c0e10
                rim: 12%
                shadow: 0, 10, 26, 45%
                box {{ from: mn.x.{s}, mn.y.{s}; size: 220, mn.h.{s}; corner: 12 }}
            }}
            box {{ from: mn.x.{s} + 12, mn.y.{s} + mn.base.{s} - 6; size: 196, 1; color: ink; opacity: 10%; show: mn.wn.{s} > 0 }}""")
        # Which rows there are: running, all four; not, Open and the pin.
        for r in range(4):
            ry = f"mn.y.{s} + mn.base.{s} + {r} * 28"
            w(f"""            zone box dmr{r}.{s} {{ from: mn.x.{s} + 6, {ry}; size: 208, 28; corner: 8; cursor: pointer; active: dmenu.{s} >= 0 and {('true' if r < 2 else f'mn.run.{s}')}; label: "A dock menu item" }}
            box {{ from: mn.x.{s} + 6, {ry}; size: 208, 28; corner: 8; color: {'#ef7a66' if r == 3 else 'ink'}; opacity: {'12%' if r == 3 else '8%'} * dmr{r}.{s}.hover; show: {('true' if r < 2 else f'mn.run.{s}')} }}""")
        lab = lambda r, t, show, col="ink": f'            text "{t}" {{ at: mn.x.{s} + 18, mn.y.{s} + mn.base.{s} + {r} * 28 + 14; anchor: left center; size: 12; weight: 500; color: {col}; show: {show} }}'
        w(lab(0, "New window", f"mn.run.{s}"))
        w(lab(0, "Open", f"not mn.run.{s}"))
        w(lab(1, "Hide", f"mn.run.{s}"))
        w(lab(1, "Take off the shore", f"not mn.run.{s} and mn.pin.{s}"))
        w(lab(1, "Keep on the shore", f"not mn.run.{s} and not mn.pin.{s}"))
        w(lab(2, "Take off the shore", f"mn.run.{s} and mn.pin.{s}"))
        w(lab(2, "Keep on the shore", f"mn.run.{s} and not mn.pin.{s}"))
        w(lab(3, "Close windows", f"mn.run.{s}", "#ef7a66"))
        w("""        }
    }""")
        w(f"""    on press dmr0.{s} while mn.run.{s} {{ dock dmk.{s} on {s} new }}
    on press dmr0.{s} while not mn.run.{s} {{ dock dmk.{s} on {s} }}
    on press dmr1.{s} while mn.run.{s} {{ dock dmk.{s} on {s} hide }}
    on press dmr1.{s} while not mn.run.{s} and mn.pin.{s} {{ dock dmk.{s} on {s} unpin }}
    on press dmr1.{s} while not mn.run.{s} and not mn.pin.{s} {{ dock dmk.{s} on {s} pin }}
    on press dmr2.{s} while mn.run.{s} and mn.pin.{s} {{ dock dmk.{s} on {s} unpin }}
    on press dmr2.{s} while mn.run.{s} and not mn.pin.{s} {{ dock dmk.{s} on {s} pin }}
    on press dmr3.{s} while mn.run.{s} {{ dock dmk.{s} on {s} close }}
    on press dmr0.{s} {{ dmenu.{s} = -1 }}
    on press dmr1.{s} {{ dmenu.{s} = -1 }}
    on press dmr2.{s} {{ dmenu.{s} = -1 }}
    on press dmr3.{s} {{ dmenu.{s} = -1 }}""")
    w("")
    return "\n".join(o)


def main():
    text = SCENE.read_text()
    lines = text.split("\n")
    a = next(i for i, l in enumerate(lines) if l.startswith(START))
    b = next(i for i, l in enumerate(lines) if l.startswith(END))
    out = lines[:a] + gen().split("\n") + lines[b:]
    SCENE.write_text("\n".join(out))
    print(f"shore: lines {a + 1}–{b} replaced by {len(gen().split(chr(10)))}", file=sys.stderr)


if __name__ == "__main__":
    main()
