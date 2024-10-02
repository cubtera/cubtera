run "setup_tests" {
  module {
    source = "./tests/setup" # Runs the setup code to generate our expected outputs
  }
}

run "verify_users" {
  command = apply # Test runs on Terraform apply, you can use `plan` if you want

  # Filename cannot change as other projects depend on it
  assert {
    condition     = strcontains(local_file.user_data.filename, run.setup_tests.expected_filename)
    error_message = "Expected: ${run.setup_tests.expected_filename} Actual: ${local_file.user_data.filename}"
  }

  # Admins are strictly controlled
  assert {
    condition     = keys(jsondecode(local_file.user_data.content)["admins"]) == run.setup_tests.expected_admins

    # Have to use `join` as only strings are supported in `error_message`
    error_message = "Expected: ${join(",",run.setup_tests.expected_admins)} Actual:${join(",", keys(jsondecode(local_file.user_data.content)["admins"]))}"
  }

  # Cannot be both user and admin
  assert {
    # To be honest this is quite a lot of function calls...
    condition     = setsubtract(keys(jsondecode(local_file.user_data.content)["users"]), run.setup_tests.expected_admins) == toset(keys(jsondecode(local_file.user_data.content)["users"]))
    error_message = "Expected: ${join(",",setsubtract(keys(jsondecode(local_file.user_data.content)["users"]), run.setup_tests.expected_admins))} Actual:${join(",",keys(jsondecode(local_file.user_data.content)["users"]))}"
  }
}