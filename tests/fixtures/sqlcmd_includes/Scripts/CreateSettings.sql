-- Create application settings
PRINT 'Creating application settings...';
IF NOT EXISTS (SELECT 1 FROM sys.objects WHERE name = 'AppSettings')
BEGIN
    CREATE TABLE [dbo].[AppSettings] (
        [Key] NVARCHAR(100) PRIMARY KEY,
        [Value] NVARCHAR(MAX)
    );
END
