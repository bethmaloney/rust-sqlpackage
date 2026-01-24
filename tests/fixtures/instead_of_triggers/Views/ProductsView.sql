-- View that will use INSTEAD OF triggers for modifications
CREATE VIEW [dbo].[ProductsView]
AS
SELECT
    [Id],
    [Name],
    [Price],
    [IsActive]
FROM [dbo].[Products]
WHERE [IsActive] = 1;
GO
