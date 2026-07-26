CREATE VIEW customer_notes AS
SELECT n.path, n.title, status.value AS status
FROM v_notes n
JOIN v_field_values kind
  ON kind.vault_name = n.vault_name
 AND kind.note_path = n.path
 AND kind.key = 'kind'
LEFT JOIN v_field_values status
  ON status.vault_name = n.vault_name
 AND status.note_path = n.path
 AND status.key = 'status'
WHERE kind.value = 'customer';

CREATE VIEW stream_rollup AS
SELECT n.path, n.title, status.value AS status, priority.value AS priority,
       customer.value AS customer
FROM v_notes n
JOIN v_field_values kind
  ON kind.vault_name = n.vault_name
 AND kind.note_path = n.path
 AND kind.key = 'kind'
 AND kind.value = 'stream'
LEFT JOIN v_field_values status
  ON status.vault_name = n.vault_name
 AND status.note_path = n.path
 AND status.key = 'status'
LEFT JOIN v_field_values priority
  ON priority.vault_name = n.vault_name
 AND priority.note_path = n.path
 AND priority.key = 'priority'
LEFT JOIN v_field_values customer
  ON customer.vault_name = n.vault_name
 AND customer.note_path = n.path
 AND customer.key = 'customers';
