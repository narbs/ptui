# Ratatui 0.29 to 0.30 Migration Notes

## Summary of Changes Required

When upgrading from ratatui 0.29 to 0.30, the following key changes are required:

1. **Text Type Mismatch**: 
   - ratatui 0.30 introduced a separation between `ratatui::text::Text` and `ratatui-core`
   - All uses of Text types must be explicitly qualified with `ratatui::text::Text`

2. **Rect Type Changes**:
   - The StatefulProtocol trait in ratatui-image v2.0 expects `ratatui::prelude::Rect`
   - May need to import via `use ratatui::prelude::*` instead of specific layout imports

3. **Dependency Versioning Issues**:
   - Some crates may have version conflicts between ratatui 0.30 and ratatui-image 2.0
   - The project build system requires careful dependency resolution

## Key Files to Modify

### src/viuer_protocol.rs
- Change imports from `ratatui::layout::Rect` to `ratatui::prelude::*`
- Update trait method signatures to match expected Rect types

### src/app.rs, src/preview.rs, src/transitions.rs  
- Qualify all Text usage with `ratatui::text::Text<'static>`

## Recommended Approach

1. First ensure dependencies are correctly resolved:
   ```
   cargo update -p ratatui-image --precise 2.0.1
   ```

2. Then make targeted fixes to the specific type mismatches shown in compilation errors.

The upgrade is feasible but requires careful attention to version compatibility and explicit type qualification.