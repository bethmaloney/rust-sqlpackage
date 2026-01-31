CREATE FUNCTION [dbo].[TableFunc1]
(
    @IsActive BIT = 1
)
RETURNS TABLE
AS
RETURN
(
    SELECT [Id], [Name], [Description], [Amount], [Quantity], [CreatedDate]
    FROM [dbo].[Table2]
    WHERE [IsActive] = @IsActive
);
GO
