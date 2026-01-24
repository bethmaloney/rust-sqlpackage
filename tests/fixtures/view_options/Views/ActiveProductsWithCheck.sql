-- View WITH CHECK OPTION (enforces filter on INSERT/UPDATE through view)
CREATE VIEW [dbo].[ActiveProductsWithCheck]
AS
SELECT
    [Id],
    [CategoryId],
    [Name],
    [Price],
    [IsActive]
FROM [dbo].[Products]
WHERE [IsActive] = 1
WITH CHECK OPTION;
GO
