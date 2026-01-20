-- Seed initial users
PRINT 'Seeding users...';
INSERT INTO [dbo].[Users] ([Username], [Email])
VALUES
    ('admin', 'admin@example.com'),
    ('testuser', 'test@example.com');
