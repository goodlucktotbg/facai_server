#!/bin/sh
echo "test"
sea-orm-cli generate entity -u mysql://root:54168ccC@localhost:3306/facai -o ./entities/src/entities --with-serde both
rm -f ./entities/src/entities/admin_role_menu.rs
rm -f ./entities/src/entities/admin_role_users.rs
rm -f ./entities/src/entities/admin_role_permissions.rs
rm -f ./entities/src/entities/admin_permission_menu.rs
rm -f ./entities/src/entities/mod.rs
rm -f ./entities/src/entities/prelude.rs
cp ./entities/mod.rs.bak ./entities/src/entities/mod.rs
cp ./entities/prelude.rs.bak ./entities/src/entities/prelude.rs
