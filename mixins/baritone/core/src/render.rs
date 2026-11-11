//! `PathRenderer`: what Baritone draws in the world each frame, as plain lines and boxes for the game to draw
//! (the path, the next one, the search's best guess so far, the blocks a path will break or place, the goal).

use crate::baritone::Baritone;
use crate::math::{BlockPos, Vec3};
use crate::pathing::goals::{Goal, GoalComposite, GoalInverted, GoalXZ, GoalYLevel};
use crate::settings::{Color, Settings};

/// One thing to draw, in world coordinates, colours `0xAARRGGBB`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Primitive {
    Line {
        from: [f64; 3],
        to: [f64; 3],
        color: u32,
        width: f32,
        through_walls: bool,
    },
    Box {
        min: [f64; 3],
        max: [f64; 3],
        color: u32,
        width: f32,
        through_walls: bool,
    },
}

fn argb(c: Color, alpha: f32) -> u32 {
    c.argb((alpha.clamp(0.0, 1.0) * 255.0) as u8)
}

/// Everything to draw this frame. `player` is where the player is drawn (for a `GoalYLevel`'s box).
pub fn primitives(b: &Baritone, player: Vec3) -> Vec<Primitive> {
    let s = &*b.bot.settings;
    let mut out = Vec::new();
    let pathing = &b.bot.pathing;
    if s.render_goal
        && let Some(goal) = pathing.goal()
    {
        draw_goal(&mut out, s, goal.as_ref(), s.color_goal_box, player);
    }
    if !s.render_path {
        return out;
    }
    let through = s.render_path_ignore_depth;
    if let Some(cur) = pathing.current() {
        draw_path(&mut out, s, cur.path().positions(), cur.position(), s.color_current_path, through);
    }
    if let Some(next) = pathing.next() {
        draw_path(&mut out, s, next.path().positions(), 0, s.color_next_path, through);
    }
    if s.render_selection_boxes {
        let boxes = s.render_selection_boxes_ignore_depth;
        for ex in pathing.current().into_iter().chain(pathing.next()) {
            for p in ex.to_break() {
                block_box(&mut out, *p, s.color_blocks_to_break, 2.0, boxes);
            }
            for p in ex.to_place() {
                block_box(&mut out, *p, s.color_blocks_to_place, 2.0, boxes);
            }
            for p in ex.to_walk_into() {
                block_box(&mut out, *p, s.color_blocks_to_walk_into, 2.0, boxes);
            }
        }
    }
    // The search under way: the best path so far, and the path to the node it looked at last.
    if let Some(p) = pathing.in_progress() {
        if let Some(best) = p.best_so_far() {
            draw_path(&mut out, s, &best, 0, s.color_best_path_so_far, through);
        }
        if let Some(recent) = p.most_recent() {
            draw_path(&mut out, s, &recent, 0, s.color_most_recent_considered, through);
            if let Some(last) = recent.last() {
                block_box(&mut out, *last, s.color_most_recent_considered, 2.0, through);
            }
        }
    }
    out
}

/// `drawPath`: straight runs joined into one line each, from `start`; with `fadePath`, fading out from 20
/// movements ahead and gone at 30.
fn draw_path(out: &mut Vec<Primitive>, s: &Settings, positions: &[BlockPos], start: usize, color: Color, through: bool) {
    let (fade_start, fade_end) = (start + 20, start + 30);
    let width = s.path_render_line_width_pixels;
    let mut i = start;
    while i + 1 < positions.len() {
        let a = positions[i];
        let mut next = i + 1;
        let mut b = positions[next];
        let dir = b.subtract(a);
        while next + 1 < positions.len()
            && (!s.fade_path || next + 1 < fade_start)
            && positions[next + 1].subtract(b) == dir
        {
            next += 1;
            b = positions[next];
        }
        let alpha = if s.fade_path && next >= fade_start {
            if next >= fade_end {
                break;
            }
            1.0 - (next - fade_start) as f32 / (fade_end - fade_start) as f32
        } else {
            1.0
        };
        let c = argb(color, 0.4 + 0.6 * alpha);
        let (ax, ay, az) = (a.x as f64 + 0.5, a.y as f64 + 0.5, a.z as f64 + 0.5);
        let (bx, by, bz) = (b.x as f64 + 0.5, b.y as f64 + 0.5, b.z as f64 + 0.5);
        let mut line = |from: [f64; 3], to: [f64; 3]| {
            out.push(Primitive::Line {
                from,
                to,
                color: c,
                width,
                through_walls: through,
            })
        };
        line([ax, ay, az], [bx, by, bz]);
        if !s.render_path_as_line {
            // `renderPathAsFrickinThingy`: a thin ribbon above the line.
            line([bx, by, bz], [bx, by + 0.03, bz]);
            line([bx, by + 0.03, bz], [ax, ay + 0.03, az]);
            line([ax, ay + 0.03, az], [ax, ay, az]);
        }
        i = next;
    }
}

fn block_box(out: &mut Vec<Primitive>, p: BlockPos, color: Color, width: f32, through: bool) {
    let e = 0.002;
    out.push(Primitive::Box {
        min: [p.x as f64 - e, p.y as f64 - e, p.z as f64 - e],
        max: [p.x as f64 + 1.0 + e, p.y as f64 + 1.0 + e, p.z as f64 + 1.0 + e],
        color: argb(color, 0.4),
        width,
        through_walls: through,
    });
}

/// `drawGoal`.
fn draw_goal(out: &mut Vec<Primitive>, s: &Settings, goal: &dyn Goal, color: Color, player: Vec3) {
    let through = s.render_goal_ignore_depth;
    let width = s.goal_render_line_width_pixels;
    let any = goal.as_any();
    if let Some(c) = any.downcast_ref::<GoalComposite>() {
        for g in &c.goals {
            draw_goal(out, s, g.as_ref(), color, player);
        }
        return;
    }
    if let Some(inv) = any.downcast_ref::<GoalInverted>() {
        draw_goal(out, s, inv.origin.as_ref(), s.color_inverted_goal_box, player);
        return;
    }
    if let Some(xz) = any.downcast_ref::<GoalXZ>() {
        // A column through the whole world.
        let (x, z) = (xz.x as f64, xz.z as f64);
        out.push(Primitive::Box {
            min: [x, -64.0, z],
            max: [x + 1.0, 320.0, z + 1.0],
            color: argb(color, 0.5),
            width,
            through_walls: through,
        });
        return;
    }
    if let Some(y) = any.downcast_ref::<GoalYLevel>() {
        let r = s.y_level_box_size;
        let y = y.level as f64;
        out.push(Primitive::Box {
            min: [player.x - r, y, player.z - r],
            max: [player.x + r, y + 1.0, player.z + r],
            color: argb(color, 0.5),
            width,
            through_walls: through,
        });
        return;
    }
    if let Some(p) = goal.render_pos() {
        // A goal for the feet or the head (two blocks) reads better as the two-block space it means.
        let height = if goal.is_in_goal(p.x, p.y - 1, p.z) { 2.0 } else { 1.0 };
        let base = if height > 1.0 { p.y as f64 - 1.0 } else { p.y as f64 };
        out.push(Primitive::Box {
            min: [p.x as f64, base, p.z as f64],
            max: [p.x as f64 + 1.0, base + height, p.z as f64 + 1.0],
            color: argb(color, 0.6),
            width,
            through_walls: through,
        });
    }
}
