---
type: note
created: 2025-01-15
---

# Unclosed Code Fence

Some text before the fence.

```sql
SELECT * FROM notes
WHERE vault_name = 'test'

This text comes after an unclosed code fence. The parser should still handle this gracefully.

More content below.
