"""Reviewed Python source replacements for the legacy SleufBase core.

Modules are added here one by one during the bytecode migration.  A module in
this package is never selected automatically; CI or a developer must explicitly
enable it through ``SLEUFBASE_MIGRATION_SOURCE_MODULES`` until the final cut-over.
"""
