-- View WITH SCHEMABINDING (prevents underlying table changes)
CREATE VIEW [dbo].[ProductsView]
WITH SCHEMABINDING
AS
SELECT
    [Id],
    [Name],
    [Price],
    [IsActive]
FROM [dbo].[Products];
GO
