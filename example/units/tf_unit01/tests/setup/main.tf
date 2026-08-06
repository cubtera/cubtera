terraform {}

# Requirement - file name cannot be changed
output "expected_filename" {
  value = "user_data.json"
}

# Requirement - admin users strictly controlled
output "expected_admins" {
  value = ["gary", "wendy"]
}