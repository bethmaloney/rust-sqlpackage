-- ALTER VIEW with SCHEMABINDING (sqlparser-rs fails on bare WITH SCHEMABINDING)
ALTER VIEW [dbo].[BoundView]
WITH SCHEMABINDING
AS
SELECT [Id], [Name] FROM [dbo].[Users];
GO

-- ALTER VIEW without SCHEMABINDING (sqlparser-rs handles this)
ALTER VIEW [dbo].[SimpleView]
AS
SELECT [OrderId], [Total] FROM [dbo].[Orders];
