-- RP-03: roles / permissions and legacy authority mapping.
CREATE TABLE IF NOT EXISTS roles (
    id INTEGER PRIMARY KEY,
    code TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    builtin INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS permissions (
    id INTEGER PRIMARY KEY,
    code TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS role_permissions (
    role_id INTEGER NOT NULL,
    permission_id INTEGER NOT NULL,
    PRIMARY KEY (role_id, permission_id),
    FOREIGN KEY (role_id) REFERENCES roles(id),
    FOREIGN KEY (permission_id) REFERENCES permissions(id)
);

CREATE TABLE IF NOT EXISTS user_roles (
    user_id INTEGER NOT NULL,
    role_id INTEGER NOT NULL,
    PRIMARY KEY (user_id, role_id),
    FOREIGN KEY (user_id) REFERENCES rp_users(id),
    FOREIGN KEY (role_id) REFERENCES roles(id)
);

INSERT OR IGNORE INTO roles (id, code, name, builtin) VALUES
    (1, 'admin', 'Administrator', 1),
    (2, 'operator', 'Operator', 1),
    (3, 'viewer', 'Viewer', 1);

INSERT OR IGNORE INTO permissions (id, code, description) VALUES
    (1,  'user:admin', 'Manage panel users'),
    (2,  'user:read', 'Read panel users'),
    (3,  'site:read', 'Read sites'),
    (4,  'site:write', 'Create/update/delete sites'),
    (5,  'file:read', 'Read managed files'),
    (6,  'file:write', 'Write managed files'),
    (7,  'app:read', 'List installable software'),
    (8,  'app:install', 'Install system software'),
    (9,  'app:uninstall', 'Uninstall system software'),
    (10, 'docker:read', 'Read Docker resources'),
    (11, 'docker:write', 'Mutate Docker resources'),
    (12, 'firewall:read', 'Read firewall rules'),
    (13, 'firewall:write', 'Change firewall rules'),
    (14, 'service:read', 'Read systemd services'),
    (15, 'service:write', 'Start/stop systemd services'),
    (16, 'process:read', 'List processes'),
    (17, 'process:write', 'Kill processes'),
    (18, 'cert:read', 'Read certificates'),
    (19, 'cert:issue', 'Issue/renew/upload certificates'),
    (20, 'cert:deploy', 'Deploy certificates / install certbot'),
    (21, 'monitor:read', 'Read live metrics'),
    (22, 'log:read', 'Read system logs'),
    (23, 'system:read', 'Read OS overview');

-- admin: every permission
INSERT OR IGNORE INTO role_permissions (role_id, permission_id)
SELECT 1, id FROM permissions;

-- operator: day-to-day ops without user admin, software install, or firewall writes
INSERT OR IGNORE INTO role_permissions (role_id, permission_id)
SELECT 2, id FROM permissions WHERE code IN (
    'user:read',
    'site:read', 'site:write',
    'file:read', 'file:write',
    'app:read',
    'docker:read', 'docker:write',
    'firewall:read',
    'service:read', 'service:write',
    'process:read', 'process:write',
    'cert:read', 'cert:issue', 'cert:deploy',
    'monitor:read', 'log:read', 'system:read'
);

-- viewer: read-only
INSERT OR IGNORE INTO role_permissions (role_id, permission_id)
SELECT 3, id FROM permissions WHERE code LIKE '%:read';

-- Map legacy authority column onto user_roles (idempotent).
INSERT OR IGNORE INTO user_roles (user_id, role_id)
SELECT id, 1 FROM rp_users WHERE authority = 'admin';

INSERT OR IGNORE INTO user_roles (user_id, role_id)
SELECT id, 3 FROM rp_users WHERE authority != 'admin';
