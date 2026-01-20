-- This view references a table that doesn't exist in the project
CREATE VIEW [dbo].[BadView]
AS
SELECT [Id], [Name]
FROM [dbo].[NonExistentTable];
