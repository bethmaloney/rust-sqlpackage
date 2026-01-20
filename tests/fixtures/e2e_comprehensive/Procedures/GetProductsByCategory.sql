CREATE PROCEDURE [dbo].[GetProductsByCategory]
    @CategoryId INT,
    @ActiveOnly BIT = 1
AS
BEGIN
    SET NOCOUNT ON;

    SELECT
        [Id],
        [SKU],
        [Name],
        [Price],
        [Quantity]
    FROM [dbo].[Products]
    WHERE [CategoryId] = @CategoryId
      AND (@ActiveOnly = 0 OR [IsActive] = 1);
END
GO
