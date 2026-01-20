CREATE PROCEDURE [dbo].[GetProducts]
    @CategoryId INT = NULL
AS
BEGIN
    SET NOCOUNT ON;

    SELECT [Id], [SKU], [Name], [Price], [Quantity]
    FROM [dbo].[Products]
    WHERE @CategoryId IS NULL OR [CategoryId] = @CategoryId;
END
GO
