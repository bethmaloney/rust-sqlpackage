CREATE FUNCTION [dbo].[GetProductsInPriceRange]
(
    @MinPrice DECIMAL(18, 2),
    @MaxPrice DECIMAL(18, 2)
)
RETURNS TABLE
AS
RETURN
(
    SELECT [Id], [SKU], [Name], [Price], [Quantity]
    FROM [dbo].[Products]
    WHERE [Price] BETWEEN @MinPrice AND @MaxPrice
      AND [IsActive] = 1
);
GO
