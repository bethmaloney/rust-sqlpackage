-- Post-deployment script with SQLCMD includes
PRINT 'Starting post-deployment...';

-- Seed data using :r includes
:r Scripts\SeedUsers.sql
:r Scripts\SeedOrders.sql

PRINT 'Post-deployment complete.';
