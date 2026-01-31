CREATE FUNCTION [dbo].[TableFunc7]
(
    @IsActive BIT = 1
)
RETURNS TABLE
AS
RETURN
(
    SELECT [Id], [Name], [Description], [Amount], [Quantity], [CreatedDate]
    FROM [dbo].[Table8]
    WHERE [IsActive] = @IsActive
);
GO
