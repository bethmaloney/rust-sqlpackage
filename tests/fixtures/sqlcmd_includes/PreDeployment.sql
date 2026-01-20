-- Pre-deployment script with SQLCMD includes
PRINT 'Starting pre-deployment...';

-- Create settings table before main schema
:r Scripts\CreateSettings.sql

PRINT 'Pre-deployment complete.';
