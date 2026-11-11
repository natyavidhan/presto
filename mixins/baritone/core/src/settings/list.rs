// Generated from Baritone's Settings.java by tools/gen_settings.py. Do not edit by hand:
// regenerate it instead, so the names, defaults and descriptions stay Baritone's.
define_settings! {
    /// Allow Baritone to break blocks
    allow_break: bool = true => "allowBreak";
    /// Blocks that baritone will be allowed to break even with allowBreak set to false
    allow_break_anyway: BlockList = BlockList::of(&[]) => "allowBreakAnyway";
    /// Allow Baritone to sprint
    allow_sprint: bool = true => "allowSprint";
    /// Allow Baritone to place blocks
    allow_place: bool = true => "allowPlace";
    /// Allow Baritone to place blocks in fluid source blocks
    allow_place_in_fluids_source: bool = true => "allowPlaceInFluidsSource";
    /// Allow Baritone to place blocks in flowing fluid
    allow_place_in_fluids_flow: bool = true => "allowPlaceInFluidsFlow";
    /// Allow Baritone to move items in your inventory to your hotbar
    allow_inventory: bool = false => "allowInventory";
    /// Wait this many ticks between InventoryBehavior moving inventory items
    ticks_between_inventory_moves: i32 = 1 => "ticksBetweenInventoryMoves";
    /// Come to a halt before doing any inventory moves. Intended for anticheat such as 2b2t
    inventory_move_only_if_stationary: bool = false => "inventoryMoveOnlyIfStationary";
    /// Disable baritone's auto-tool at runtime, but still assume that another mod will provide auto tool functionality
    ///
    /// Specifically, path calculation will still assume that an auto tool will run at execution time, even though
    /// Baritone itself will not do that.
    assume_external_auto_tool: bool = false => "assumeExternalAutoTool";
    /// Automatically select the best available tool
    auto_tool: bool = true => "autoTool";
    /// It doesn't actually take twenty ticks to place a block, this cost is so high
    /// because we want to generally conserve blocks which might be limited.
    ///
    /// Decrease to make Baritone more often consider paths that would require placing blocks
    block_placement_penalty: f64 = 20.0 => "blockPlacementPenalty";
    /// This is just a tiebreaker to make it less likely to break blocks if it can avoid it.
    /// For example, fire has a break cost of 0, this makes it nonzero, so all else being equal
    /// it will take an otherwise equivalent route that doesn't require it to put out fire.
    block_break_additional_penalty: f64 = 2.0 => "blockBreakAdditionalPenalty";
    /// Additional penalty for hitting the space bar (ascend, pillar, or parkour) because it uses hunger
    jump_penalty: f64 = 2.0 => "jumpPenalty";
    /// Walking on water uses up hunger really quick, so penalize it
    walk_on_water_one_penalty: f64 = 3.0 => "walkOnWaterOnePenalty";
    /// Don't allow breaking blocks next to liquids.
    ///
    /// Enable if you have mods adding custom fluid physics.
    strict_liquid_check: bool = false => "strictLiquidCheck";
    /// Allow Baritone to fall arbitrary distances and place a water bucket beneath it.
    /// Reliability: questionable.
    allow_water_bucket_fall: bool = true => "allowWaterBucketFall";
    /// Allow Baritone to assume it can walk on still water just like any other block.
    /// This functionality is assumed to be provided by a separate library that might have imported Baritone.
    ///
    /// Note: This will prevent some usage of the frostwalker enchantment, like pillaring up from water.
    assume_walk_on_water: bool = false => "assumeWalkOnWater";
    /// If you have Fire Resistance and Jesus then I guess you could turn this on lol
    assume_walk_on_lava: bool = false => "assumeWalkOnLava";
    /// Assume step functionality; don't jump on an Ascend.
    assume_step: bool = false => "assumeStep";
    /// Assume safe walk functionality; don't sneak on a backplace traverse.
    ///
    /// Warning: if you do something janky like sneak-backplace from an ender chest, if this is true
    /// it won't sneak right click, it'll just right click, which means it'll open the chest instead of placing
    /// against it. That's why this defaults to off.
    assume_safe_walk: bool = false => "assumeSafeWalk";
    /// If true, parkour is allowed to make jumps when standing on blocks at the maximum height, so player feet is y=256
    ///
    /// Defaults to false because this fails on constantiam. Please let me know if this is ever disabled. Please.
    allow_jump_at_build_limit: bool = false => "allowJumpAtBuildLimit";
    /// Just here so mods that use the API don't break. Does nothing.
    allow_jump_at256: bool = false => "allowJumpAt256";
    /// This should be monetized it's so good
    ///
    /// Defaults to true, but only actually takes effect if allowParkour is also true
    allow_parkour_ascend: bool = true => "allowParkourAscend";
    /// Allow descending diagonally
    ///
    /// Safer than allowParkour yet still slightly unsafe, can make contact with unchecked adjacent blocks, so it's unsafe in the nether.
    ///
    /// For a generic "take some risks" mode I'd turn on this one, parkour, and parkour place.
    allow_diagonal_descend: bool = false => "allowDiagonalDescend";
    /// Allow diagonal ascending
    ///
    /// Actually pretty safe, much safer than diagonal descend tbh
    allow_diagonal_ascend: bool = false => "allowDiagonalAscend";
    /// Allow mining the block directly beneath its feet
    ///
    /// Turn this off to force it to make more staircases and less shafts
    allow_downward: bool = true => "allowDownward";
    /// Blocks that Baritone is allowed to place (as throwaway, for sneak bridging, pillaring, etc.)
    acceptable_throwaway_items: ItemList = ItemList::of(&["minecraft:dirt", "minecraft:cobblestone", "minecraft:netherrack", "minecraft:stone"]) => "acceptableThrowawayItems";
    /// Blocks that Baritone will attempt to avoid (Used in avoidance)
    blocks_to_avoid: BlockList = BlockList::of(&["minecraft:tripwire"]) => "blocksToAvoid";
    /// Blocks that Baritone is not allowed to break
    blocks_to_disallow_breaking: BlockList = BlockList::of(&[]) => "blocksToDisallowBreaking";
    /// blocks that baritone shouldn't break, but can if it needs to.
    blocks_to_avoid_breaking: BlockList = BlockList::of(&["minecraft:crafting_table", "minecraft:furnace", "minecraft:chest", "minecraft:trapped_chest"]) => "blocksToAvoidBreaking";
    /// this multiplies the break speed, if set above 1 it's "encourage breaking" instead
    avoid_breaking_multiplier: f64 = 0.1 => "avoidBreakingMultiplier";
    /// A list of blocks to be treated as if they're air.
    ///
    /// If a schematic asks for air at a certain position, and that position currently contains a block on this list, it will be treated as correct.
    build_ignore_blocks: BlockList = BlockList::of(&[]) => "buildIgnoreBlocks";
    /// A list of blocks to be treated as correct.
    ///
    /// If a schematic asks for any block on this list at a certain position, it will be treated as correct, regardless of what it currently is.
    build_skip_blocks: BlockList = BlockList::of(&[]) => "buildSkipBlocks";
    /// A mapping of blocks to blocks treated as correct in their position
    ///
    /// If a schematic asks for a block on this mapping, all blocks on the mapped list will be accepted at that location as well
    ///
    /// Syntax same as <a href="https://baritone.leijurv.com/baritone/api/Settings.html#buildSubstitutes">buildSubstitutes</a>
    build_valid_substitutes: BlockMap = BlockMap::default() => "buildValidSubstitutes";
    /// A mapping of blocks to blocks to be built instead
    ///
    /// If a schematic asks for a block on this mapping, Baritone will place the first placeable block in the mapped list
    ///
    /// Usage Syntax:
    /// <pre>
    ///      sourceblockA->blockToSubstituteA1,blockToSubstituteA2,...blockToSubstituteAN,sourceBlockB->blockToSubstituteB1,blockToSubstituteB2,...blockToSubstituteBN,...sourceBlockX->blockToSubstituteX1,blockToSubstituteX2...blockToSubstituteXN
    /// </pre>
    /// Example:
    /// <pre>
    ///     stone->cobblestone,andesite,oak_planks->birch_planks,acacia_planks,glass
    /// </pre>
    build_substitutes: BlockMap = BlockMap::default() => "buildSubstitutes";
    /// A list of blocks to become air
    ///
    /// If a schematic asks for a block on this list, only air will be accepted at that location (and nothing on buildIgnoreBlocks)
    ok_if_air: BlockList = BlockList::of(&[]) => "okIfAir";
    /// If this is true, the builder will treat all non-air blocks as correct. It will only place new blocks.
    build_ignore_existing: bool = false => "buildIgnoreExisting";
    /// If this is true, the builder will ignore directionality of certain blocks like glazed terracotta.
    build_ignore_direction: bool = false => "buildIgnoreDirection";
    /// A list of names of block properties the builder will ignore.
    build_ignore_properties: StringList = StringList::default() => "buildIgnoreProperties";
    /// If this setting is true, Baritone will never break a block that is adjacent to an unsupported falling block.
    ///
    /// I.E. it will never trigger cascading sand / gravel falls
    avoid_updating_falling_blocks: bool = true => "avoidUpdatingFallingBlocks";
    /// Enables some more advanced vine features. They're honestly just gimmicks and won't ever be needed in real
    /// pathing scenarios. And they can cause Baritone to get trapped indefinitely in a strange scenario.
    ///
    /// Almost never turn this on lol
    allow_vines: bool = false => "allowVines";
    /// Slab behavior is complicated, disable this for higher path reliability. Leave enabled if you have bottom slabs
    /// everywhere in your base.
    allow_walk_on_bottom_slab: bool = true => "allowWalkOnBottomSlab";
    /// You know what it is
    ///
    /// But it's very unreliable and falls off when cornering like all the time so.
    ///
    /// It also overshoots the landing pretty much always (making contact with the next block over), so be careful
    allow_parkour: bool = false => "allowParkour";
    /// Actually pretty reliable.
    ///
    /// Doesn't make it any more dangerous compared to just normal allowParkour th
    allow_parkour_place: bool = false => "allowParkourPlace";
    /// For example, if you have Mining Fatigue or Haste, adjust the costs of breaking blocks accordingly.
    consider_potion_effects: bool = true => "considerPotionEffects";
    /// Sprint and jump a block early on ascends wherever possible
    sprint_ascends: bool = true => "sprintAscends";
    /// If we overshoot a traverse and end up one block beyond the destination, mark it as successful anyway.
    ///
    /// This helps with speed exceeding 20m/s
    overshoot_traverse: bool = true => "overshootTraverse";
    /// When breaking blocks for a movement, wait until all falling blocks have settled before continuing
    pause_mining_for_falling_blocks: bool = true => "pauseMiningForFallingBlocks";
    /// How many ticks between right clicks are allowed. Default in game is 4
    right_click_speed: i32 = 4 => "rightClickSpeed";
    /// How many degrees to randomize the yaw every tick. Set to 0 to disable
    random_looking113: f64 = 2.0 => "randomLooking113";
    /// Block reach distance
    block_reach_distance: f32 = 4.5 => "blockReachDistance";
    /// How many ticks between breaking a block and starting to break the next block. Default in game is 6 ticks.
    /// Values under 1 will be clamped. The delay only applies to non-instant (1-tick) breaks.
    block_break_speed: i32 = 6 => "blockBreakSpeed";
    /// How many degrees to randomize the pitch and yaw every tick. Set to 0 to disable
    random_looking: f64 = 0.01 => "randomLooking";
    /// This is the big A* setting.
    /// As long as your cost heuristic is an *underestimate*, it's guaranteed to find you the best path.
    /// 3.5 is always an underestimate, even if you are sprinting.
    /// If you're walking only (with allowSprint off) 4.6 is safe.
    /// Any value below 3.5 is never worth it. It's just more computation to find the same path, guaranteed.
    /// (specifically, it needs to be strictly slightly less than ActionCosts.WALK_ONE_BLOCK_COST, which is about 3.56)
    ///
    /// Setting it at 3.57 or above with sprinting, or to 4.64 or above without sprinting, will result in
    /// faster computation, at the cost of a suboptimal path. Any value above the walk / sprint cost will result
    /// in it going straight at its goal, and not investigating alternatives, because the combined cost / heuristic
    /// metric gets better and better with each block, instead of slightly worse.
    ///
    /// Finding the optimal path is worth it, so it's the default.
    cost_heuristic: f64 = 3.563 => "costHeuristic";
    /// The maximum number of times it will fetch outside loaded or cached chunks before assuming that
    /// pathing has reached the end of the known area, and should therefore stop.
    pathing_max_chunk_border_fetch: i32 = 50 => "pathingMaxChunkBorderFetch";
    /// Set to 1.0 to effectively disable this feature
    ///
    /// @see <a href="https://github.com/cabaletta/baritone/issues/18">Issue #18</a>
    backtrack_cost_favoring_coefficient: f64 = 0.5 => "backtrackCostFavoringCoefficient";
    /// Toggle the following 4 settings
    ///
    /// They have a noticeable performance impact, so they default off
    ///
    /// Specifically, building up the avoidance map on the main thread before pathing starts actually takes a noticeable
    /// amount of time, especially when there are a lot of mobs around, and your game jitters for like 200ms while doing so
    avoidance: bool = false => "avoidance";
    /// Set to 1.0 to effectively disable this feature
    ///
    /// Set below 1.0 to go out of your way to walk near mob spawners
    mob_spawner_avoidance_coefficient: f64 = 2.0 => "mobSpawnerAvoidanceCoefficient";
    /// Distance to avoid mob spawners.
    mob_spawner_avoidance_radius: i32 = 16 => "mobSpawnerAvoidanceRadius";
    /// Set to 1.0 to effectively disable this feature
    ///
    /// Set below 1.0 to go out of your way to walk near mobs
    mob_avoidance_coefficient: f64 = 1.5 => "mobAvoidanceCoefficient";
    /// Distance to avoid mobs.
    mob_avoidance_radius: i32 = 8 => "mobAvoidanceRadius";
    /// When running a goto towards a container block (chest, ender chest, furnace, etc),
    /// right click and open it once you arrive.
    right_click_container_on_arrival: bool = true => "rightClickContainerOnArrival";
    /// When running a goto towards a nether portal block, walk all the way into the portal
    /// instead of stopping one block before.
    enter_portal: bool = true => "enterPortal";
    /// Don't repropagate cost improvements below 0.01 ticks. They're all just floating point inaccuracies,
    /// and there's no point.
    minimum_improvement_repropagation: bool = true => "minimumImprovementRepropagation";
    /// After calculating a path (potentially through cached chunks), artificially cut it off to just the part that is
    /// entirely within currently loaded chunks. Improves path safety because cached chunks are heavily simplified.
    ///
    /// This is much safer to leave off now, and makes pathing more efficient. More explanation in the issue.
    ///
    /// @see <a href="https://github.com/cabaletta/baritone/issues/114">Issue #114</a>
    cutoff_at_load_boundary: bool = false => "cutoffAtLoadBoundary";
    /// If a movement's cost increases by more than this amount between calculation and execution (due to changes
    /// in the environment / world), cancel and recalculate
    max_cost_increase: f64 = 10.0 => "maxCostIncrease";
    /// Stop 5 movements before anything that made the path COST_INF.
    /// For example, if lava has spread across the path, don't walk right up to it then recalculate, it might
    /// still be spreading lol
    cost_verification_lookahead: i32 = 5 => "costVerificationLookahead";
    /// Static cutoff factor. 0.9 means cut off the last 10% of all paths, regardless of chunk load state
    path_cutoff_factor: f64 = 0.9 => "pathCutoffFactor";
    /// Only apply static cutoff for paths of at least this length (in terms of number of movements)
    path_cutoff_minimum_length: i32 = 30 => "pathCutoffMinimumLength";
    /// Start planning the next path once the remaining movements tick estimates sum up to less than this value
    planning_tick_lookahead: i32 = 150 => "planningTickLookahead";
    /// Default size of the Long2ObjectOpenHashMap used in pathing
    pathing_map_default_size: i32 = 1024 => "pathingMapDefaultSize";
    /// Load factor coefficient for the Long2ObjectOpenHashMap used in pathing
    ///
    /// Decrease for faster map operations, but higher memory usage
    pathing_map_load_factor: f32 = 0.75 => "pathingMapLoadFactor";
    /// How far are you allowed to fall onto solid ground (without a water bucket)?
    /// 3 won't deal any damage. But if you just want to get down the mountain quickly and you have
    /// Feather Falling IV, you might set it a bit higher, like 4 or 5.
    max_fall_height_no_water: i32 = 3 => "maxFallHeightNoWater";
    /// How far are you allowed to fall onto solid ground (with a water bucket)?
    /// It's not that reliable, so I've set it below what would kill an unarmored player (23)
    max_fall_height_bucket: i32 = 20 => "maxFallHeightBucket";
    /// Is it okay to sprint through a descend followed by a diagonal?
    /// The player overshoots the landing, but not enough to fall off. And the diagonal ensures that there isn't
    /// lava or anything that's !canWalkInto in that space, so it's technically safe, just a little sketchy.
    ///
    /// Note: this is *not* related to the allowDiagonalDescend setting, that is a completely different thing.
    allow_overshoot_diagonal_descend: bool = true => "allowOvershootDiagonalDescend";
    /// If your goal is a GoalBlock in an unloaded chunk, assume it's far enough away that the Y coord
    /// doesn't matter yet, and replace it with a GoalXZ to the same place before calculating a path.
    /// Once a segment ends within chunk load range of the GoalBlock, it will go back to normal behavior
    /// of considering the Y coord. The reasoning is that if your X and Z are 10,000 blocks away,
    /// your Y coordinate's accuracy doesn't matter at all until you get much much closer.
    simplify_unloaded_y_coord: bool = true => "simplifyUnloadedYCoord";
    /// Whenever a block changes, repack the whole chunk that it's in
    repack_on_any_block_change: bool = true => "repackOnAnyBlockChange";
    /// If a movement takes this many ticks more than its initial cost estimate, cancel it
    movement_timeout_ticks: i32 = 100 => "movementTimeoutTicks";
    /// Pathing ends after this amount of time, but only if a path has been found
    ///
    /// If no valid path (length above the minimum) has been found, pathing continues up until the failure timeout
    primary_timeout_ms: i64 = 500 => "primaryTimeoutMS";
    /// Pathing can never take longer than this, even if that means failing to find any path at all
    failure_timeout_ms: i64 = 2000 => "failureTimeoutMS";
    /// Planning ahead while executing a segment ends after this amount of time, but only if a path has been found
    ///
    /// If no valid path (length above the minimum) has been found, pathing continues up until the failure timeout
    plan_ahead_primary_timeout_ms: i64 = 4000 => "planAheadPrimaryTimeoutMS";
    /// Planning ahead while executing a segment can never take longer than this, even if that means failing to find any path at all
    plan_ahead_failure_timeout_ms: i64 = 5000 => "planAheadFailureTimeoutMS";
    /// For debugging, consider nodes much much slower
    slow_path: bool = false => "slowPath";
    /// Milliseconds between each node
    slow_path_time_delay_ms: i64 = 100 => "slowPathTimeDelayMS";
    /// The alternative timeout number when slowPath is on
    slow_path_timeout_ms: i64 = 40000 => "slowPathTimeoutMS";
    /// allows baritone to save bed waypoints when interacting with beds
    do_bed_waypoints: bool = true => "doBedWaypoints";
    /// allows baritone to save death waypoints
    do_death_waypoints: bool = true => "doDeathWaypoints";
    /// The big one. Download all chunks in simplified 2-bit format and save them for better very-long-distance pathing.
    chunk_caching: bool = true => "chunkCaching";
    /// On save, delete from RAM any cached regions that are more than 1024 blocks away from the player
    ///
    /// Temporarily disabled
    ///
    /// Temporarily reenabled
    ///
    /// @see <a href="https://github.com/cabaletta/baritone/issues/248">Issue #248</a>
    prune_regions_from_ram: bool = true => "pruneRegionsFromRAM";
    /// The chunk packer queue can never grow to larger than this, if it does, the oldest chunks are discarded
    ///
    /// The newest chunks are kept, so that if you're moving in a straight line quickly then stop, your immediate render distance is still included
    chunk_packer_queue_max_size: i32 = 2000 => "chunkPackerQueueMaxSize";
    /// Fill in blocks behind you
    backfill: bool = false => "backfill";
    /// Shows popup message in the upper right corner, similarly to when you make an advancement
    log_as_toast: bool = false => "logAsToast";
    /// Print all the debug messages to chat
    chat_debug: bool = false => "chatDebug";
    /// Allow chat based control of Baritone. Most likely should be disabled when Baritone is imported for use in
    /// something else
    chat_control: bool = true => "chatControl";
    /// Some clients like Impact try to force chatControl to off, so here's a second setting to do it anyway
    chat_control_anyway: bool = false => "chatControlAnyway";
    /// Render the path
    render_path: bool = true => "renderPath";
    /// Render the path as a line instead of a frickin thingy
    render_path_as_line: bool = false => "renderPathAsLine";
    /// Render the goal
    render_goal: bool = true => "renderGoal";
    /// Render the goal as a sick animated thingy instead of just a box
    /// (also controls animation of GoalXZ if `#renderGoalXZBeacon` is enabled)
    render_goal_animated: bool = true => "renderGoalAnimated";
    /// Render selection boxes
    render_selection_boxes: bool = true => "renderSelectionBoxes";
    /// Ignore depth when rendering the goal
    render_goal_ignore_depth: bool = true => "renderGoalIgnoreDepth";
    /// Renders X/Z type Goals as a no-depth beacon beam instead of the full-height goal box.
    render_goal_xz_beacon: bool = false => "renderGoalXZBeacon";
    /// Ignore depth when rendering the selection boxes (to break, to place, to walk into)
    render_selection_boxes_ignore_depth: bool = true => "renderSelectionBoxesIgnoreDepth";
    /// Ignore depth when rendering the path
    render_path_ignore_depth: bool = true => "renderPathIgnoreDepth";
    /// Line width of the path when rendered, in pixels
    path_render_line_width_pixels: f32 = 5.0 => "pathRenderLineWidthPixels";
    /// Line width of the goal when rendered, in pixels
    goal_render_line_width_pixels: f32 = 3.0 => "goalRenderLineWidthPixels";
    /// Start fading out the path at 20 movements ahead, and stop rendering it entirely 30 movements ahead.
    /// Improves FPS.
    fade_path: bool = false => "fadePath";
    /// Move without having to force the client-sided rotations
    free_look: bool = true => "freeLook";
    /// Break and place blocks without having to force the client-sided rotations. Requires `#freeLook`.
    block_free_look: bool = false => "blockFreeLook";
    /// Automatically elytra fly without having to force the client-sided rotations.
    elytra_free_look: bool = true => "elytraFreeLook";
    /// Forces the client-sided yaw rotation to an average of the last `#smoothLookTicks` of server-sided rotations.
    smooth_look: bool = false => "smoothLook";
    /// Same as `#smoothLook` but for elytra flying.
    elytra_smooth_look: bool = false => "elytraSmoothLook";
    /// The number of ticks to average across for `#smoothLook`;
    smooth_look_ticks: i32 = 5 => "smoothLookTicks";
    /// When true, the player will remain with its existing look direction as often as possible.
    /// Although, in some cases this can get it stuck, hence this setting to disable that behavior.
    remain_with_existing_look_direction: bool = true => "remainWithExistingLookDirection";
    /// Will cause some minor behavioral differences to ensure that Baritone works on anticheats.
    ///
    /// At the moment this will silently set the player's rotations when using freeLook so you're not sprinting in
    /// directions other than forward, which is picken up by more "advanced" anticheats like AAC, but not NCP.
    anti_cheat_compatibility: bool = true => "antiCheatCompatibility";
    /// Exclusively use cached chunks for pathing
    ///
    /// Never turn this on
    path_through_cached_only: bool = false => "pathThroughCachedOnly";
    /// Continue sprinting while in water
    sprint_in_water: bool = true => "sprintInWater";
    /// When GetToBlockProcess or MineProcess fails to calculate a path, instead of just giving up, mark the closest instance
    /// of that block as "unreachable" and go towards the next closest. GetToBlock expands this search to the whole "vein"; MineProcess does not.
    /// This is because MineProcess finds individual impossible blocks (like one block in a vein that has gravel on top then lava, so it can't break)
    /// Whereas GetToBlock should blacklist the whole "vein" if it can't get to any of them.
    blacklist_closest_on_failure: bool = true => "blacklistClosestOnFailure";
    /// 😎 Render cached chunks as semitransparent. Doesn't work with OptiFine 😭 Rarely randomly crashes, see <a href="https://github.com/cabaletta/baritone/issues/327">this issue</a>.
    ///
    /// Can be very useful on servers with low render distance. After enabling, you may need to reload the world in order for it to have an effect
    /// (e.g. disconnect and reconnect, enter then exit the nether, die and respawn, etc). This may literally kill your FPS and CPU because
    /// every chunk gets recompiled twice as much as normal, since the cached version comes into range, then the normal one comes from the server for real.
    ///
    /// Note that flowing water is cached as AVOID, which is rendered as lava. As you get closer, you may therefore see lava falls being replaced with water falls.
    ///
    /// SOLID is rendered as stone in the overworld, netherrack in the nether, and end stone in the end
    render_cached_chunks: bool = false => "renderCachedChunks";
    /// 0.0f = not visible, fully transparent (instead of setting this to 0, turn off renderCachedChunks)
    /// 1.0f = fully opaque
    cached_chunks_opacity: f32 = 0.5 => "cachedChunksOpacity";
    /// Whether or not to allow you to run Baritone commands with the prefix
    prefix_control: bool = true => "prefixControl";
    /// The command prefix for chat control
    prefix: String = String::from("#") => "prefix";
    /// Use a short Baritone prefix [B] instead of [Baritone] when logging to chat
    short_baritone_prefix: bool = false => "shortBaritonePrefix";
    /// Use a modern message tag instead of a prefix when logging to chat
    use_message_tag: bool = false => "useMessageTag";
    /// Echo commands to chat when they are run
    echo_commands: bool = true => "echoCommands";
    /// Censor coordinates in goals and block positions
    censor_coordinates: bool = false => "censorCoordinates";
    /// Censor arguments to ran commands, to hide, for example, coordinates to #goal
    censor_ran_commands: bool = false => "censorRanCommands";
    /// Stop using tools just before they are going to break.
    item_saver: bool = false => "itemSaver";
    /// Durability to leave on the tool when using itemSaver
    item_saver_threshold: i32 = 10 => "itemSaverThreshold";
    /// Always prefer silk touch tools over regular tools. This will not sacrifice speed, but it will always prefer silk
    /// touch tools over other tools of the same speed. This includes always choosing ANY silk touch tool over your hand.
    prefer_silk_touch: bool = false => "preferSilkTouch";
    /// Don't stop walking forward when you need to break blocks in your way
    walk_while_breaking: bool = true => "walkWhileBreaking";
    /// When a new segment is calculated that doesn't overlap with the current one, but simply begins where the current segment ends,
    /// splice it on and make a longer combined path. If this setting is off, any planned segment will not be spliced and will instead
    /// be the "next path" in PathingBehavior, and will only start after this one ends. Turning this off hurts planning ahead,
    /// because the next segment will exist even if it's very short.
    ///
    /// @see #planningTickLookahead
    splice_path: bool = true => "splicePath";
    /// If we are more than 300 movements into the current path, discard the oldest segments, as they are no longer useful
    max_path_history_length: i32 = 300 => "maxPathHistoryLength";
    /// If the current path is too long, cut off this many movements from the beginning.
    path_history_cutoff_amount: i32 = 50 => "pathHistoryCutoffAmount";
    /// Rescan for the goal once every 5 ticks.
    /// Set to 0 to disable.
    mine_goal_update_interval: i32 = 5 => "mineGoalUpdateInterval";
    /// After finding this many instances of the target block in the cache, it will stop expanding outward the chunk search.
    max_cached_world_scan_count: i32 = 10 => "maxCachedWorldScanCount";
    /// Mine will not scan for or remember more than this many target locations.
    /// Note that the number of locations retrieved from cache is additionaly
    /// limited by `#maxCachedWorldScanCount`.
    mine_max_ore_locations_count: i32 = 64 => "mineMaxOreLocationsCount";
    /// Sets the minimum y level whilst mining - set to 0 to turn off.
    /// if world has negative y values, subtract the min world height to get the value to put here
    min_y_level_while_mining: i32 = 0 => "minYLevelWhileMining";
    /// Sets the maximum y level to mine ores at.
    max_y_level_while_mining: i32 = 2031 => "maxYLevelWhileMining";
    /// This will only allow baritone to mine exposed ores, can be used to stop ore obfuscators on servers that use them.
    allow_only_exposed_ores: bool = false => "allowOnlyExposedOres";
    /// When allowOnlyExposedOres is enabled this is the distance around to search.
    ///
    /// It is recommended to keep this value low, as it dramatically increases calculation times.
    allow_only_exposed_ores_distance: i32 = 1 => "allowOnlyExposedOresDistance";
    /// When GetToBlock or non-legit Mine doesn't know any locations for the desired block, explore randomly instead of giving up.
    explore_for_blocks: bool = true => "exploreForBlocks";
    /// While exploring the world, offset the closest unloaded chunk by this much in both axes.
    ///
    /// This can result in more efficient loading, if you set this to the render distance.
    world_exploring_chunk_offset: i32 = 0 => "worldExploringChunkOffset";
    /// Take the 10 closest chunks, even if they aren't strictly tied for distance metric from origin.
    explore_chunk_set_minimum_size: i32 = 10 => "exploreChunkSetMinimumSize";
    /// Attempt to maintain Y coordinate while exploring
    ///
    /// -1 to disable
    explore_maintain_y: i32 = 64 => "exploreMaintainY";
    /// Replant normal Crops while farming and leave cactus and sugarcane to regrow
    replant_crops: bool = true => "replantCrops";
    /// Replant nether wart while farming. This setting only has an effect when replantCrops is also enabled
    replant_nether_wart: bool = false => "replantNetherWart";
    /// When enabled, farming will be restricted to the current selection.
    farm_using_selection: bool = false => "farmUsingSelection";
    /// Farming will scan for at most this many blocks.
    farm_max_scan_size: i32 = 256 => "farmMaxScanSize";
    /// When the cache scan gives less blocks than the maximum threshold (but still above zero), scan the main world too.
    ///
    /// Only if you have a beefy CPU and automatically mine blocks that are in cache
    extend_cache_on_threshold: bool = false => "extendCacheOnThreshold";
    /// Don't consider the next layer in builder until the current one is done
    build_in_layers: bool = false => "buildInLayers";
    /// false = build from bottom to top
    ///
    /// true = build from top to bottom
    layer_order: bool = false => "layerOrder";
    /// How high should the individual layers be?
    layer_height: i32 = 1 => "layerHeight";
    /// Start building the schematic at a specific layer.
    /// Can help on larger builds when schematic wants to break things its already built
    start_at_layer: i32 = 0 => "startAtLayer";
    /// If a layer is unable to be constructed, just skip it.
    skip_failed_layers: bool = false => "skipFailedLayers";
    /// Only build the selected part of schematics
    build_only_selection: bool = false => "buildOnlySelection";
    /// How far to move before repeating the build. 0 to disable repeating on a certain axis, 0,0,0 to disable entirely
    build_repeat: [i32; 3] = [0, 0, 0] => "buildRepeat";
    /// How many times to buildrepeat. -1 for infinite.
    build_repeat_count: i32 = -1 => "buildRepeatCount";
    /// Don't notify schematics that they are moved.
    /// e.g. replacing will replace the same spots for every repetition
    /// Mainly for backward compatibility.
    build_repeat_sneaky: bool = true => "buildRepeatSneaky";
    /// Allow standing above a block while mining it, in BuilderProcess
    ///
    /// Experimental
    break_from_above: bool = false => "breakFromAbove";
    /// As well as breaking from above, set a goal to up and to the side of all blocks to break.
    ///
    /// Never turn this on without also turning on breakFromAbove.
    goal_break_from_above: bool = false => "goalBreakFromAbove";
    /// Build in map art mode, which makes baritone only care about the top block in each column
    map_art_mode: bool = false => "mapArtMode";
    /// Override builder's behavior to not attempt to correct blocks that are currently water
    ok_if_water: bool = false => "okIfWater";
    /// The set of incorrect blocks can never grow beyond this size
    incorrect_size: i32 = 100 => "incorrectSize";
    /// Multiply the cost of breaking a block that's correct in the builder's schematic by this coefficient
    break_correct_block_penalty_multiplier: f64 = 10.0 => "breakCorrectBlockPenaltyMultiplier";
    /// Multiply the cost of placing a block that's incorrect in the builder's schematic by this coefficient
    place_incorrect_block_penalty_multiplier: f64 = 2.0 => "placeIncorrectBlockPenaltyMultiplier";
    /// When this setting is true, build a schematic with the highest X coordinate being the origin, instead of the lowest
    schematic_orientation_x: bool = false => "schematicOrientationX";
    /// When this setting is true, build a schematic with the highest Y coordinate being the origin, instead of the lowest
    schematic_orientation_y: bool = false => "schematicOrientationY";
    /// When this setting is true, build a schematic with the highest Z coordinate being the origin, instead of the lowest
    schematic_orientation_z: bool = false => "schematicOrientationZ";
    /// Rotates the schematic before building it.
    /// Possible values are
    ///
    ///   NONE - No rotation
    ///   CLOCKWISE_90 - Rotate 90° clockwise
    ///   CLOCKWISE_180 - Rotate 180° clockwise
    ///   COUNTERCLOCKWISE_90 - Rotate 270° clockwise
    build_schematic_rotation: SchematicRotation = SchematicRotation::None => "buildSchematicRotation";
    /// Mirrors the schematic before building it.
    /// Possible values are
    ///
    ///   FRONT_BACK - mirror the schematic along its local x axis
    ///   LEFT_RIGHT - mirror the schematic along its local z axis
    build_schematic_mirror: SchematicMirror = SchematicMirror::None => "buildSchematicMirror";
    /// The fallback used by the build command when no extension is specified. This may be useful if schematics of a
    /// particular format are used often, and the user does not wish to have to specify the extension with every usage.
    schematic_fallback_extension: String = String::from("schematic") => "schematicFallbackExtension";
    /// Distance to scan every tick for updates. Expanding this beyond player reach distance (i.e. setting it to 6 or above)
    /// is only necessary in very large schematics where rescanning the whole thing is costly.
    builder_tick_scan_radius: i32 = 5 => "builderTickScanRadius";
    /// While mining, should it also consider dropped items of the correct type as a pathing destination (as well as ore blocks)?
    mine_scan_dropped_items: bool = true => "mineScanDroppedItems";
    /// While mining, wait this number of milliseconds after mining an ore to see if it will drop an item
    /// instead of immediately going onto the next one
    ///
    /// Thanks Louca
    mine_drop_loiter_duration_ms_thanks_louca: i64 = 250 => "mineDropLoiterDurationMSThanksLouca";
    /// Trim incorrect positions too far away, helps performance but hurts reliability in very large schematics
    distance_trim: bool = true => "distanceTrim";
    /// Cancel the current path if the goal has changed, and the path originally ended in the goal but doesn't anymore.
    ///
    /// Currently only runs when either MineBehavior or FollowBehavior is active.
    ///
    /// For example, if Baritone is doing "mine iron_ore", the instant it breaks the ore (and it becomes air), that location
    /// is no longer a goal. This means that if this setting is true, it will stop there. If this setting were off, it would
    /// continue with its path, and walk into that location. The tradeoff is if this setting is true, it mines ores much faster
    /// since it doesn't waste any time getting into locations that no longer contain ores, but on the other hand, it misses
    /// some drops, and continues on without ever picking them up.
    ///
    /// Also on cosmic prisons this should be set to true since you don't actually mine the ore it just gets replaced with stone.
    cancel_on_goal_invalidation: bool = true => "cancelOnGoalInvalidation";
    /// The "axis" command (aka GoalAxis) will go to a axis, or diagonal axis, at this Y level.
    axis_height: i32 = 120 => "axisHeight";
    /// Disconnect from the server upon arriving at your goal
    disconnect_on_arrival: bool = false => "disconnectOnArrival";
    /// Disallow MineBehavior from using X-Ray to see where the ores are. Turn this option on to force it to mine "legit"
    /// where it will only mine an ore once it can actually see it, so it won't do or know anything that a normal player
    /// couldn't. If you don't want it to look like you're X-Raying, turn this on
    /// This will always explore, regardless of exploreForBlocks
    legit_mine: bool = false => "legitMine";
    /// What Y level to go to for legit strip mining
    legit_mine_y_level: i32 = -59 => "legitMineYLevel";
    /// Magically see ores that are separated diagonally from existing ores. Basically like mining around the ores that it finds
    /// in case there's one there touching it diagonally, except it checks it un-legit-ly without having the mine blocks to see it.
    /// You can decide whether this looks plausible or not.
    ///
    /// This is disabled because it results in some weird behavior. For example, it can """see""" the top block of a vein of iron_ore
    /// through a lava lake. This isn't an issue normally since it won't consider anything touching lava, so it just ignores it.
    /// However, this setting expands that and allows it to see the entire vein so it'll mine under the lava lake to get the iron that
    /// it can reach without mining blocks adjacent to lava. This really defeats the purpose of legitMine since a player could never
    /// do that lol, so thats one reason why its disabled
    legit_mine_include_diagonals: bool = false => "legitMineIncludeDiagonals";
    /// When mining block of a certain type, try to mine two at once instead of one.
    /// If the block above is also a goal block, set GoalBlock instead of GoalTwoBlocks
    /// If the block below is also a goal block, set GoalBlock to the position one down instead of GoalTwoBlocks
    force_internal_mining: bool = true => "forceInternalMining";
    /// Modification to the previous setting, only has effect if forceInternalMining is true
    /// If true, only apply the previous setting if the block adjacent to the goal isn't air.
    internal_mining_air_exception: bool = true => "internalMiningAirException";
    /// The actual GoalNear is set this distance away from the entity you're following
    ///
    /// For example, set followOffsetDistance to 5 and followRadius to 0 to always stay precisely 5 blocks north of your follow target.
    follow_offset_distance: f64 = 0.0 => "followOffsetDistance";
    /// The actual GoalNear is set in this direction from the entity you're following. This value is in degrees.
    follow_offset_direction: f32 = 0.0 => "followOffsetDirection";
    /// The radius (for the GoalNear) of how close to your target position you actually have to be
    follow_radius: i32 = 3 => "followRadius";
    /// The maximum distance to the entity you're following
    follow_target_max_distance: i32 = 0 => "followTargetMaxDistance";
    /// Turn this on if your exploration filter is enormous, you don't want it to check if it's done,
    /// and you are just fine with it just hanging on completion
    disable_completion_check: bool = false => "disableCompletionCheck";
    /// Cached chunks (regardless of if they're in RAM or saved to disk) expire and are deleted after this number of seconds
    /// -1 to disable
    ///
    /// I would highly suggest leaving this setting disabled (-1).
    ///
    /// The only valid reason I can think of enable this setting is if you are extremely low on disk space and you play on multiplayer,
    /// and can't take (average) 300kb saved for every 512x512 area. (note that more complicated terrain is less compressible and will take more space)
    ///
    /// However, simply discarding old chunks because they are old is inadvisable. Baritone is extremely good at correcting
    /// itself and its paths as it learns new information, as new chunks load. There is no scenario in which having an
    /// incorrect cache can cause Baritone to get stuck, take damage, or perform any action it wouldn't otherwise, everything
    /// is rechecked once the real chunk is in range.
    ///
    /// Having a robust cache greatly improves long distance pathfinding, as it's able to go around large scale obstacles
    /// before they're in render distance. In fact, when the chunkCaching setting is disabled and Baritone starts anew
    /// every time, or when you enter a completely new and very complicated area, it backtracks far more often because it
    /// has to build up that cache from scratch. But after it's gone through an area just once, the next time will have zero
    /// backtracking, since the entire area is now known and cached.
    cached_chunks_expiry_seconds: i64 = -1 => "cachedChunksExpirySeconds";
    /// Print out ALL command exceptions as a stack trace to stdout, even simple syntax errors
    verbose_command_exceptions: bool = false => "verboseCommandExceptions";
    /// The size of the box that is rendered when the current goal is a GoalYLevel
    y_level_box_size: f64 = 15.0 => "yLevelBoxSize";
    /// The color of the current path
    color_current_path: Color = Color::rgb(255, 0, 0) => "colorCurrentPath";
    /// The color of the next path
    color_next_path: Color = Color::rgb(255, 0, 255) => "colorNextPath";
    /// The color of the blocks to break
    color_blocks_to_break: Color = Color::rgb(255, 0, 0) => "colorBlocksToBreak";
    /// The color of the blocks to place
    color_blocks_to_place: Color = Color::rgb(0, 255, 0) => "colorBlocksToPlace";
    /// The color of the blocks to walk into
    color_blocks_to_walk_into: Color = Color::rgb(255, 0, 255) => "colorBlocksToWalkInto";
    /// The color of the best path so far
    color_best_path_so_far: Color = Color::rgb(0, 0, 255) => "colorBestPathSoFar";
    /// The color of the path to the most recent considered node
    color_most_recent_considered: Color = Color::rgb(0, 255, 255) => "colorMostRecentConsidered";
    /// The color of the goal box
    color_goal_box: Color = Color::rgb(0, 255, 0) => "colorGoalBox";
    /// The color of the goal box when it's inverted
    color_inverted_goal_box: Color = Color::rgb(255, 0, 0) => "colorInvertedGoalBox";
    /// The color of all selections
    color_selection: Color = Color::rgb(0, 255, 255) => "colorSelection";
    /// The color of the selection pos 1
    color_selection_pos1: Color = Color::rgb(0, 0, 0) => "colorSelectionPos1";
    /// The color of the selection pos 2
    color_selection_pos2: Color = Color::rgb(255, 200, 0) => "colorSelectionPos2";
    /// The opacity of the selection. 0 is completely transparent, 1 is completely opaque
    selection_opacity: f32 = 0.5 => "selectionOpacity";
    /// Line width of the goal when rendered, in pixels
    selection_line_width: f32 = 2.0 => "selectionLineWidth";
    /// Render selections
    render_selection: bool = true => "renderSelection";
    /// Ignore depth when rendering selections
    render_selection_ignore_depth: bool = true => "renderSelectionIgnoreDepth";
    /// Render selection corners
    render_selection_corners: bool = true => "renderSelectionCorners";
    /// Use sword to mine.
    use_sword_to_mine: bool = true => "useSwordToMine";
    /// Desktop notifications
    desktop_notifications: bool = false => "desktopNotifications";
    /// Desktop notification on path complete
    notification_on_path_complete: bool = true => "notificationOnPathComplete";
    /// Desktop notification on farm fail
    notification_on_farm_fail: bool = true => "notificationOnFarmFail";
    /// Desktop notification on build finished
    notification_on_build_finished: bool = true => "notificationOnBuildFinished";
    /// Desktop notification on explore finished
    notification_on_explore_finished: bool = true => "notificationOnExploreFinished";
    /// Desktop notification on mine fail
    notification_on_mine_fail: bool = true => "notificationOnMineFail";
    /// The number of ticks of elytra movement to simulate while firework boost is not active. Higher values are
    /// computationally more expensive.
    elytra_simulation_ticks: i32 = 20 => "elytraSimulationTicks";
    /// The maximum allowed deviation in pitch from a direct line-of-sight to the flight target. Higher values are
    /// computationally more expensive.
    elytra_pitch_range: i32 = 25 => "elytraPitchRange";
    /// The minimum speed that the player can drop to (in blocks/tick) before a firework is automatically deployed.
    elytra_firework_speed: f64 = 1.2 => "elytraFireworkSpeed";
    /// The delay after the player's position is set-back by the server that a firework may be automatically deployed.
    /// Value is in ticks.
    elytra_firework_setback_use_delay: i32 = 15 => "elytraFireworkSetbackUseDelay";
    /// The minimum padding value that is added to the player's hitbox when considering which point to fly to on the
    /// path. High values can result in points not being considered which are otherwise safe to fly to. Low values can
    /// result in flight paths which are extremely tight, and there's the possibility of crashing due to getting too low
    /// to the ground.
    elytra_minimum_avoidance: f64 = 0.2 => "elytraMinimumAvoidance";
    /// If enabled, avoids using fireworks when descending along the flight path.
    elytra_conserve_fireworks: bool = false => "elytraConserveFireworks";
    /// Renders the raytraces that are performed by the elytra fly calculation.
    elytra_render_raytraces: bool = false => "elytraRenderRaytraces";
    /// Renders the raytraces that are used in the hitbox part of the elytra fly calculation.
    /// Requires `#elytraRenderRaytraces`.
    elytra_render_hitbox_raytraces: bool = false => "elytraRenderHitboxRaytraces";
    /// Renders the best elytra flight path that was simulated each tick.
    elytra_render_simulation: bool = true => "elytraRenderSimulation";
    /// Automatically path to and jump off of ledges to initiate elytra flight when grounded.
    elytra_auto_jump: bool = false => "elytraAutoJump";
    /// The seed used to generate chunks for long distance elytra path-finding in the nether.
    /// Defaults to 2b2t's nether seed.
    elytra_nether_seed: i64 = 146008555100680 => "elytraNetherSeed";
    /// Whether nether-pathfinder should generate terrain based on `#elytraNetherSeed`.
    /// If false all chunks that haven't been loaded are assumed to be air.
    elytra_predict_terrain: bool = false => "elytraPredictTerrain";
    /// Automatically swap the current elytra with a new one when the durability gets too low
    elytra_auto_swap: bool = true => "elytraAutoSwap";
    /// The minimum durability an elytra can have before being swapped
    elytra_minimum_durability: i32 = 5 => "elytraMinimumDurability";
    /// The minimum fireworks before landing early for safety
    elytra_min_fireworks_before_landing: i32 = 5 => "elytraMinFireworksBeforeLanding";
    /// Automatically land when elytra is almost out of durability, or almost out of fireworks
    elytra_allow_emergency_land: bool = true => "elytraAllowEmergencyLand";
    /// Time between culling far away chunks from the nether pathfinder chunk cache
    elytra_time_between_cache_cull_secs: i64 = 180 => "elytraTimeBetweenCacheCullSecs";
    /// Maximum distance chunks can be before being culled from the nether pathfinder chunk cache
    elytra_cache_cull_distance: i32 = 5000 => "elytraCacheCullDistance";
    /// Should elytra consider nether brick a valid landing block
    elytra_allow_land_on_nether_fortress: bool = false => "elytraAllowLandOnNetherFortress";
    /// Has the user read and understood the elytra terms and conditions
    elytra_terms_accepted: bool = false => "elytraTermsAccepted";
    /// Verbose chat logging in elytra mode
    elytra_chat_spam: bool = false => "elytraChatSpam";
    /// May reduce memory usage by using a custom allocator for pathfinding
    elytra_custom_allocator: bool = true => "elytraCustomAllocator";
    /// Allow the pathfinder to attempt flight in tighter spaces, useful in caves but can be dangerous.
    elytra_allow_tight_spaces: bool = false => "elytraAllowTightSpaces";
    /// Allow the pathfinder to fly above y 128 in the nether.
    elytra_allow_above_roof: bool = false => "elytraAllowAboveRoof";
    /// Allow the pathfinder to access the baritone cache to improve pathing
    elytra_use_cache: bool = true => "elytraUseCache";
    /// Allow the pathfinder to fly above the build limit in the overworld and end.
    elytra_allow_above_build_limit: bool = true => "elytraAllowAboveBuildLimit";
    /// Minimum distance in blocks of an elytra trip before the pathfinder will try to fly above build limit. (Minimum: 32). Requires `#elytraAllowAboveBuildLimit` to be enabled.
    elytra_long_distance_threshold: i32 = 500 => "elytraLongDistanceThreshold";
    /// Sneak when magma blocks are under feet
    allow_walk_on_magma_blocks: bool = false => "allowWalkOnMagmaBlocks";
}
