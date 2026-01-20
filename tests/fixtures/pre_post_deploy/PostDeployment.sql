-- Post-deployment script
PRINT 'Deployment complete.';

-- Insert seed data
INSERT INTO [dbo].[Table1] ([Id], [Name])
VALUES (1, 'Initial Record');
