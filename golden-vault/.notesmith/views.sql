CREATE VIEW customer_notes AS
SELECT n.path, n.title, customer.value AS customer, status.value AS status
FROM v_notes n
JOIN v_fields note_type
  ON note_type.vault_name = n.vault_name
 AND note_type.note_path = n.path
 AND note_type.key = 'type'
LEFT JOIN v_fields customer
  ON customer.vault_name = n.vault_name
 AND customer.note_path = n.path
 AND customer.key = 'customer'
LEFT JOIN v_fields status
  ON status.vault_name = n.vault_name
 AND status.note_path = n.path
 AND status.key = 'status'
WHERE note_type.value = 'customer';
