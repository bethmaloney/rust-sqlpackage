CREATE FUNCTION [dbo].[TableFunc5]
(
    @IsActive BIT = 1
)
RETURNS TABLE
AS
RETURN
(
    SELECT [Id], [Name], [Description], [Amount], [Quantity], [CreatedDate]
    FROM [dbo].[Table6]
    WHERE [IsActive] = @IsActive
);
GO
